import collections
import logging
import os
import textwrap
import time
import typing

import discord
import graphviz
from discord.ext import commands


log = logging.getLogger("polly")

# In order of precedence.
edge_styles = [
    ("cohab", {
        "dir": "none",
        "penwidth": "3",
    }),
    ("fwb", {
        "dir": "none",
    }),
    ("crush", {
        "style": "dashed",
    }),
    ("friend", {
        "dir": "none",
        "style": "dotted",
        "len": "2",
    }),
]


def edge_attrs(annotation, back_annotation=None, bidirectional=False):
    # For available attributes, see http://graphviz.org/doc/info/attrs.html
    attrs = {}

    for (ann, style_attrs) in edge_styles:
        if ann in (annotation, back_annotation):
            attrs.update(style_attrs)
            break

    if "crush" in (annotation, back_annotation):
        if annotation == back_annotation:
            attrs["dir"] = "both"
        elif annotation == "crush":
            attrs["dir"] = "forward"
        else:
            attrs["dir"] = "back"
    elif bidirectional:
        attrs["dir"] = "none"

    return attrs


class Connections(commands.Cog):
    def __init__(self, bot, db_conn, out_dir):
        self.bot = bot
        self.db_conn = db_conn
        self.out_dir = out_dir

        try:
            os.mkdir(out_dir)
        except FileExistsError:
            pass

        with self.db_conn:
            self.db_conn.execute("""
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

    @commands.command(
        brief="Add a connection between you and someone else in this server",
        help=textwrap.dedent("""
            Add a connection between you and someone else in this server.

            The annotation can be any text, but there are some special values which are processed by the graph command:
            {}
        """).strip().format(
            "\n".join(s[0] for s in edge_styles),
        ),
    )
    async def connect(self, ctx: commands.Context, member: discord.Member, annotation: typing.Optional[str] = None):
        with self.db_conn:
            self.db_conn.execute(
                "replace into connections values (?, ?, ?, ?)",
                (
                    ctx.guild.id,
                    ctx.author.id,
                    member.id,
                    annotation,
                )
            )
        await ctx.send(f"New connection between {ctx.author.mention} and {member.mention}")

    @commands.command(help="Remove a connection between you and someone else in this server")
    async def disconnect(self, ctx: commands.Context, member: discord.Member):
        with self.db_conn:
            n = self.db_conn.execute(
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

    @commands.command(help="Remove all your connections in this server")
    async def disconnect_all(self, ctx: commands.Context):
        with self.db_conn:
            n = self.db_conn.execute(
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
        with self.db_conn:
            n = self.db_conn.execute(
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

    @commands.command(help="Draw a graph of connections")
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
            connections_out = self.db_conn.execute(
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
            connections_in = self.db_conn.execute(
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
            node_attrs = {
                "label": "",
            }

            user = ctx.guild.get_member(user_id)

            # Something is wrong with the user cache. It should populate from
            # guild events, but as a fallback we sometimes have to make the
            # API request directly.
            if user is None:
                log.warn(f"falling back to fetch_user for {user_id}")
                user = await ctx.bot.fetch_user(user_id)

            if user is not None:
                node_attrs["label"] = user.display_name
                if user.id == member.id:
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
                    **edge_attrs(
                        edges[user_id][to_user_id],
                        edges[to_user_id].get(user_id),
                        bidirectional,
                    ),
                )

        out_file = graph.render(cleanup=True)
        await ctx.send(file=discord.File(out_file))
        os.remove(out_file)
