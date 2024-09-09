db["league_info"].deleteMany({});
db["games"].deleteMany({});
db["events"].deleteMany({});

db["league_info"].insertOne({
    "first_unreviewed_event_number": 0,
    "available_game_id": 0,
    "available_event_number": 0,
})
