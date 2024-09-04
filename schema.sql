CREATE TABLE IF NOT EXISTS players
(
    guild_id    BIGINT       NOT NULL,
    player_id   INT          NOT NULL,
    player_name VARCHAR(100) NOT NULL,
    PRIMARY KEY (guild_id, player_id),
    UNIQUE (guild_id, player_name)
);

CREATE TABLE IF NOT EXISTS player_discord
(
    guild_id        BIGINT NOT NULL,
    player_id       INT    NOT NULL,
    discord_user_id BIGINT NOT NULL,
    FOREIGN KEY (guild_id, player_id) REFERENCES players (guild_id, player_id)
);

CREATE INDEX discord_user_id ON player_discord (discord_user_id, guild_id);
