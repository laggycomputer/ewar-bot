CREATE TABLE IF NOT EXISTS players
(
    player_id   INT          NOT NULL,
    player_name VARCHAR(100) NOT NULL,
    PRIMARY KEY (player_id),
    UNIQUE (player_name)
);

CREATE TABLE IF NOT EXISTS player_discord
(
    player_id       INT    NOT NULL,
    discord_user_id BIGINT NOT NULL,
    FOREIGN KEY (player_id) REFERENCES players (player_id),
    UNIQUE (player_id, discord_user_id)
);

CREATE INDEX discord_user_id ON player_discord (discord_user_id);
