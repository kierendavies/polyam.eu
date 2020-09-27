resource discord_role_everyone everyone {
  server_id   = discord_server.server.id
  permissions = data.discord_permission.read_only.allow_bits
}

resource discord_role admin {
  server_id   = discord_server.server.id
  name        = "admin"
  permissions = data.discord_permission.allow_all.allow_bits
  hoist       = true
  mentionable = true
}

resource discord_member_roles admin {
  for_each = toset([
    "190427277002145793", # Kieren
    "190871761196154881", # Nyx
  ])
  server_id = discord_server.server.id
  user_id   = each.key
  role {
    role_id = discord_role.admin.id
  }
}

resource discord_role member {
  server_id   = discord_server.server.id
  name        = "member"
  permissions = data.discord_permission.read_only.allow_bits
  hoist       = true
  mentionable = false
}

resource discord_role verified {
  server_id   = discord_server.server.id
  name        = "verified"
  mentionable = false
}
