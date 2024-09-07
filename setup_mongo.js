db["league_info"].deleteMany({});
db["games"].deleteMany({});

db["league_info"].insertOne({
    "last_not_approved": 0,
    "last_not_submitted": 0,
    "last_free_event_number": 0,
})
