import random

from discord.ext import commands


class Bubblewrap(commands.Cog):

    @staticmethod
    def generate(size: int = 5):
        if size < 1:
            raise ValueError("value is less than 1")

        max_value: int = 25
        if size >= max_value:
            raise ValueError("value of '" + str(max_value) + "' is exceeded, got '" + str(size) + "'")

        r: int = (random.randint(0, (size * size - 1)))

        ded = (r % size, int(r / size))
        return "\n".join(
            "".join(
                "||ded||" if (x, y) == ded else "||pop||"
                for x in range(size)
            )
            for y in range(size)
        )

    @commands.command(help="Bubble wrap!")
    async def bubblewrap(self, ctx: commands.Context, size: int = 5):
        text = self.generate(size)

        await ctx.send(text)
