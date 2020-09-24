resource discord_category_channel server {
  server_id  = discord_server.server.id
  name       = "Server"
  depends_on = [discord_server.server]
}

resource discord_channel_permission cat_server {
  channel_id   = discord_category_channel.server.id
  type         = "role"
  overwrite_id = discord_role_everyone.everyone.id
  allow        = data.discord_permission.read_only.allow_bits
  depends_on   = [discord_category_channel.server]
}

resource discord_text_channel rules {
  server_id                = discord_server.server.id
  name                     = "rules"
  category                 = discord_category_channel.server.id
  sync_perms_with_category = false
  depends_on               = [discord_category_channel.server]
}

resource discord_channel_permission rules_everyone {
  channel_id   = discord_text_channel.rules.id
  type         = "role"
  overwrite_id = discord_role_everyone.everyone.id
  allow        = data.discord_permission.read_only.allow_bits
  depends_on   = [discord_text_channel.rules]
}
