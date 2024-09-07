const exists = db["league_info"].findOne();
if (!exists) {
    db["league_info"].insertOne({
        "last_not_approved": 0,
        "last_not_submitted": 0,
    })
}
