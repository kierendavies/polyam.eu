resource discord_invite invite {
  channel_id = discord_text_channel.rules.id
  max_age    = 0
  max_uses   = 0
}

output invite_url {
  value = "https://discord.gg/${discord_invite.invite.id}"
}
