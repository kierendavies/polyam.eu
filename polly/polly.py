import argparse
import collections
import logging
import os
import pprint
import sqlite3
import sys
import time
import traceback
import typing

import discord
import graphviz
from discord.ext import commands


log = logging.getLogger("polly")


class Bot(commands.Bot):
    def __init__(self):
        super().__init__(self.when_mentioned_or_dm)

    @staticmethod
    def when_mentioned_or_dm(bot, message):
        if message.guild is None:
            # Allow empty prefix in DM.
            return commands.when_mentioned(bot, message) + [""]
        else:
            return commands.when_mentioned(bot, message)

    async def on_command(self, ctx):
        try:
            can_run = await ctx.command.can_run(ctx)
        except:
            can_run = False
        log.debug(
            f"command name={repr(ctx.command.qualified_name)}"
            f" guild={repr(ctx.guild.name if ctx.guild else None)}"
            f" user={repr(str(ctx.author))}"
            f" can_run={can_run}"
            f" raw={repr(ctx.message.content)}"
        )

    async def on_command_error(self, ctx, exception):
        await ctx.send(f"Something went wrong :slight_frown:\n```\n{str(exception)}\n```")
        log.debug("command error", exc_info=exception)


class Connections(commands.Cog):
    def __init__(self, bot, db, out_dir):
        self.bot = bot
        self.db = db
        self.out_dir = out_dir

        try:
            os.mkdir(out_dir)
        except FileExistsError:
            pass

        with self.db:
            self.db.execute("""
                create table if not exists connections (
                    guild_id int8 not null,
                    from_user_id int8 not null,
                    to_user_id int8 not null,
                    annotation text,
                    primary key (guild_id, from_user_id, to_user_id)
                )
            """)

    async def cog_check(self, ctx):
        if ctx.guild is None:
            raise commands.NoPrivateMessage()
        return True

    @commands.command()
    async def connect(self, ctx: commands.Context, member: discord.Member, annotation: typing.Optional[str] = None):
        with self.db:
            self.db.execute(
                "replace into connections values (?, ?, ?, ?)",
                (
                    ctx.guild.id,
                    ctx.author.id,
                    member.id,
                    annotation,
                )
            )
        await ctx.send(f"New connection between {ctx.author.mention} and {member.mention}")

    @commands.command()
    async def disconnect(self, ctx: commands.Context, member: discord.Member):
        with self.db:
            n = self.db.execute(
                """
                    delete from connections
                    where guild_id = ? and (
                        (from_user_id = ? and to_user_id = ?) or
                        (from_user_id = ? and to_user_id = ?)
                    )
                """,
                (
                    ctx.guild.id,
                    ctx.author.id, member.id,
                    member.id, ctx.author.id,
                )
            ).rowcount
        await ctx.send(f"Removed {n} connection(s) from {ctx.author.mention}")

    @commands.command()
    async def disconnect_all(self, ctx: commands.Context):
        with self.db:
            n = self.db.execute(
                """
                    delete from connections
                    where guild_id = ? and (
                        from_user_id = ? or
                        to_user_id = ?
                    )
                """,
                (
                    ctx.guild.id,
                    ctx.author.id,
                    ctx.author.id,
                )
            ).rowcount
        await ctx.send(f"Removed {n} connection(s) from {ctx.author.mention}")

    @commands.command(hidden=True)
    @commands.has_guild_permissions(ban_members=True)
    async def disconnect_all_id(self, ctx: commands.Context, user_id: int):
        with self.db:
            n = self.db.execute(
                """
                    delete from connections
                    where guild_id = ? and (
                        from_user_id = ? or
                        to_user_id = ?
                    )
                """,
                (
                    ctx.guild.id,
                    user_id,
                    user_id,
                )
            ).rowcount
        await ctx.send(f"Removed {n} connection(s)")

    @commands.command()
    async def graph(self, ctx: commands.Context, member: typing.Optional[discord.Member] = None, radius: int = 1):
        if member is None:
            member = ctx.author

        edges = {}

        dist = {member.id: 0}
        queue = collections.deque([member.id])
        while queue:
            user_id = queue.popleft()
            if user_id not in edges:
                edges[user_id] = {}

            # Outbound connections
            connections_out = db.execute(
                "select to_user_id, annotation from connections where guild_id = ? and from_user_id = ?",
                (
                    ctx.guild.id,
                    user_id,
                )
            ).fetchall()
            for (to_user_id, annotation) in connections_out:
                d = dist.get(to_user_id, dist[user_id] + 1)
                if d > radius:
                    continue
                edges[user_id][to_user_id] = annotation
                if to_user_id not in dist:
                    dist[to_user_id] = d
                    queue.append(to_user_id)

            # Inbound connections
            connections_in = db.execute(
                "select from_user_id, annotation from connections where guild_id = ? and to_user_id = ?",
                (
                    ctx.guild.id,
                    user_id,
                )
            ).fetchall()
            for (from_user_id, annotation) in connections_in:
                d = dist.get(from_user_id, dist[user_id] + 1)
                if d > radius:
                    continue
                if from_user_id not in edges:
                    edges[from_user_id] = {}
                edges[from_user_id][user_id] = annotation
                if from_user_id not in dist:
                    dist[from_user_id] = d
                    queue.append(from_user_id)

        font_name = "sans-serif"
        graph = graphviz.Digraph(
            filename=f"connections-{member.id}-{radius}-{int(time.time())}",
            directory=self.out_dir,
            format="png",
            engine="neato",
            graph_attr={
                "fontname": font_name,
                "overlap": "ortho",
            },
            node_attr={
                "fontname": font_name,
            },
            edge_attr={
                "fontname": font_name,
            },
        )

        for user_id in edges:
            node_member = ctx.guild.get_member(user_id)
            node_attrs = {
                "label": "",
            }
            if node_member:
                node_attrs["label"] = node_member.display_name
                if node_member.id == member.id:
                    node_attrs["peripheries"] = "2"
                    node_attrs["color"] = "black:black"
            graph.node(
                str(user_id),
                **node_attrs,
            )

            for to_user_id in edges[user_id]:
                bidirectional = False
                if to_user_id in edges and user_id in edges[to_user_id]:
                    # Only add one instance of each bidirectional edge.
                    if to_user_id < user_id:
                        continue
                    bidirectional = True

                graph.edge(
                    str(user_id),
                    str(to_user_id),
                    **Connections.edge_attrs(
                        edges[user_id][to_user_id],
                        edges[to_user_id].get(user_id),
                        bidirectional,
                    ),
                )

        out_file = graph.render(cleanup=True)
        await ctx.send(file=discord.File(out_file))
        os.remove(out_file)

    @staticmethod
    def edge_attrs(annotation, back_annotation=None, bidirectional=False):
        # For available attributes, see http://graphviz.org/doc/info/attrs.html

        attrs = {}

        if "cohab" in (annotation, back_annotation):
            attrs["dir"] = "none"
            attrs["penwidth"] = "3"
        elif "fwb" in (annotation, back_annotation):
            attrs["dir"] = "none"
        elif "crush" in (annotation, back_annotation):
            attrs["style"] = "dashed"
            if annotation == back_annotation:
                attrs["dir"] = "both"
            elif annotation == "crush":
                attrs["dir"] = "forward"
            else:
                attrs["dir"] = "back"
        elif "friend" in (annotation, back_annotation):
            attrs["dir"] = "none"
            attrs["style"] = "dotted"
            attrs["len"] = "2"
        elif bidirectional:
            attrs["dir"] = "none"

        return attrs


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    log.setLevel(logging.DEBUG)

    parser = argparse.ArgumentParser()
    parser.add_argument("--token")
    parser.add_argument("--db", default="polly.db")
    parser.add_argument("--out-dir", default="out")
    args = parser.parse_args()

    bot = Bot()
    db = sqlite3.connect(args.db)
    bot.add_cog(Connections(bot, db, args.out_dir))

    bot.run(args.token)
