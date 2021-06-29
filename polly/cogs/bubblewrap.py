import logging

from discord.ext import commands


class Bubblewrap(commands.Cog):
    @commands.command(help="Bubble wrap!")
    async def bubblewrap(self, ctx: commands.Context, size: int = 5):
        ded = (random.randint(0, size - 1), random.randint(0, size - 1))
        text = "\n".join(
            "".join(
                "||ded||" if (x, y) == ded else "||pop||"
                for x in range(size)
            )
            for y in range(size)
        )

        await ctx.send(format_users(text))
