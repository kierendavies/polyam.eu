import argparse
import logging
import sqlite3

from discord.ext import commands

import cogs

log = logging.getLogger("polly")


class Bot(commands.Bot):
    def __init__(self, db, out_dir):
        super().__init__(self.when_mentioned_or_dm)

        self.db_conn = sqlite3.connect(db)
        self.out_dir = out_dir

        self.add_cog(cogs.Bubblewrap())
        self.add_cog(cogs.Connections(self.db_conn, self.out_dir))
        self.add_cog(cogs.Users())

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


if __name__ == "__main__":
    logging.basicConfig(
        level=logging.INFO,
        format="{asctime} {levelname} {name} {message}",
        style="{",
    )
    log.setLevel(logging.DEBUG)

    parser = argparse.ArgumentParser()
    parser.add_argument("--token")
    parser.add_argument("--db", default="polly.db")
    parser.add_argument("--out-dir", default="out")
    args = parser.parse_args()

    bot = Bot(args.db, args.out_dir)
    bot.run(args.token)
