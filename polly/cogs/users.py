import logging

from discord.ext import commands
import discord
import discord.utils

log = logging.getLogger("polly")


def format_users(users):
    if not users:
        return "No users found"
    return "```\n" + "\n".join(f"{u.name}#{u.discriminator}" for u in users) + "\n```"

class Users(commands.Cog):
    def __init__(self):
        pass

    async def cog_check(self, ctx):
        if ctx.guild is None:
            raise commands.NoPrivateMessage()
        return True

    @commands.command(help="List users who have the given role")
    async def with_role(self, ctx: commands.Context, role: discord.Role):
        users = []
        async for u in ctx.guild.fetch_members(limit=None):
            if not u.bot and role in u.roles:
                users.append(u)
        await ctx.send(format_users(users))

    @commands.command(help="List users who do not have the given role")
    async def without_role(self, ctx: commands.Context, role: discord.Role):
        users = []
        async for u in ctx.guild.fetch_members(limit=None):
            if not u.bot and role not in u.roles:
                users.append(u)
        await ctx.send(format_users(users))