data discord_permission allow_all {
  create_instant_invite = "allow"
  kick_members          = "allow"
  ban_members           = "allow"
  administrator         = "allow"
  manage_channels       = "allow"
  manage_guild          = "allow"
  add_reactions         = "allow"
  view_audit_log        = "allow"
  priority_speaker      = "allow"
  stream                = "allow"
  view_channel          = "allow"
  send_messages         = "allow"
  send_tts_messages     = "allow"
  manage_messages       = "allow"
  embed_links           = "allow"
  attach_files          = "allow"
  read_message_history  = "allow"
  mention_everyone      = "allow"
  use_external_emojis   = "allow"
  # view_guild_insights = "allow"
  connect          = "allow"
  speak            = "allow"
  mute_members     = "allow"
  deafen_members   = "allow"
  move_members     = "allow"
  use_vad          = "allow"
  change_nickname  = "allow"
  manage_nicknames = "allow"
  manage_roles     = "allow"
  manage_webhooks  = "allow"
  manage_emojis    = "allow"
}

data discord_permission read_only {
  view_channel         = "allow"
  read_message_history = "allow"
}
