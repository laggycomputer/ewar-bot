db["league_info"].deleteMany({});
db["games"].deleteMany({});
db["events"].deleteMany({});

db["league_info"].insertOne({
    "last_not_approved_game": 0,
    "available_game_id": 0,
    "available_event_number": 0,
})
