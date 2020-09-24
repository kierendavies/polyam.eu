resource discord_message rules {
  channel_id = discord_text_channel.rules.id
  # content    = "This is a message"
  embed {
    title = "Rules"
    description = <<-END
      Click the :white_check_mark: below this message to agree to these rules and gain access to the rest of the server.

      1. You must be at least 18 years old to be a member of this server.
      2. Always follow the Code of Conduct.
      3. Speak English in the common channels.
    END
  }
  depends_on = [discord_text_channel.rules]
}

resource discord_message code_of_conduct_pledge {
  channel_id = discord_text_channel.rules.id
  embed {
    title = "Code of Conduct - Our Pledge"
    description = <<-END
      We as members and leaders pledge to make participation in our community a harassment-free experience for everyone, regardless of age, body size, visible or invisible disability, ethnicity, sex characteristics, gender identity and expression, level of experience, education, socio-economic status, nationality, personal appearance, race, religion, or sexual identity and orientation.

      We pledge to act and interact in ways that contribute to an open, welcoming, diverse, inclusive, and healthy community.
    END
  }
  depends_on = [discord_message.rules]
}

resource discord_message code_of_conduct_standards {
  channel_id = discord_text_channel.rules.id
  embed {
    title = "Code of Conduct - Our Standards"
    description = <<-END
      Examples of behavior that contributes to a positive environment for our community include:
      • Demonstrating empathy and kindness toward other people
      • Being respectful of differing opinions, viewpoints, and experiences
      • Giving and gracefully accepting constructive feedback
      • Accepting responsibility and apologizing to those affected by our mistakes, and learning from the experience
      • Focusing on what is best not just for us as individuals, but for the overall community

      Examples of unacceptable behavior include:
      • Unwanted sexual attention or advances of any kind
      • Trolling, insulting or derogatory comments, and personal or political attacks
      • Public or private harassment
      • Publishing others’ private information, such as a physical or email address, without their explicit permission
      • Other conduct which could reasonably be considered inappropriate
    END
  }
  depends_on = [discord_message.code_of_conduct_pledge]
}

resource discord_message code_of_conduct_enforcement {
  channel_id = discord_text_channel.rules.id
  embed {
    title = "Code of Conduct - Enforcement"
    description = <<-END
      Community leaders are responsible for clarifying and enforcing our standards of acceptable behavior and will take appropriate and fair corrective action in response to any behavior that they deem inappropriate, threatening, offensive, or harmful.

      Community leaders have the right and responsibility to remove messages that are not aligned to this Code of Conduct, and will communicate reasons for moderation decisions when appropriate.

      Instances of abusive, harassing, or otherwise unacceptable behavior may be reported to the community leaders in the #deleted-channel channel. All complaints will be reviewed and investigated promptly and fairly.

      All community leaders are obligated to respect the privacy and security of the reporter of any incident.
    END
  }
  depends_on = [discord_message.code_of_conduct_pledge]
}

resource discord_message code_of_conduct_enforcement_guidelines {
  channel_id = discord_text_channel.rules.id
  embed {
    title = "Code of Conduct - Enforcement Guidelines"
    description = <<-END
      Community leaders will follow these Community Impact Guidelines in determining the consequences for any action they deem in violation of this Code of Conduct:

      1. _Correction_
      Community Impact: Use of inappropriate language or other behavior deemed unwelcome in the community.
      Consequence: A warning from community leaders, providing clarity around the nature of the violation and an explanation of why the behavior was inappropriate. A public apology may be requested.

      2. _Warning_
      Community Impact: A violation through a single incident or series of actions.
      Consequence: A warning with consequences for continued behavior. No interaction with the people involved, including unsolicited interaction with those enforcing the Code of Conduct, for a specified period of time. This includes avoiding interactions in community spaces as well as external channels like social media. Violating these terms may lead to a temporary or permanent ban.

      3. _Temporary Ban_
      Community Impact: A serious violation of community standards, including sustained inappropriate behavior.
      Consequence: A temporary ban from any sort of interaction or public communication with the community for a specified period of time. No public or private interaction with the people involved, including unsolicited interaction with those enforcing the Code of Conduct, is allowed during this period. Violating these terms may lead to a permanent ban.

      4. _Permanent Ban_
      Community Impact: Demonstrating a pattern of violation of community standards, including sustained inappropriate behavior, harassment of an individual, or aggression toward or disparagement of classes of individuals.
      Consequence: A permanent ban from any sort of public interaction within the community.
    END
  }
  depends_on = [discord_message.code_of_conduct_enforcement]
}

resource discord_message code_of_conduct_attribution {
  channel_id = discord_text_channel.rules.id
  embed {
    title = "Code of Conduct - Attribution"
    description = <<-END
      This Code of Conduct is adapted from the Contributor Covenant, version 2.0, available at https://www.contributor-covenant.org/version/2/0/code_of_conduct.html.
    END
  }
  depends_on = [discord_message.code_of_conduct_enforcement_guidelines]
}
