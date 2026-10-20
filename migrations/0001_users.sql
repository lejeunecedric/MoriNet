-- MoriNet users: email + bcrypt password hash + role.
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'reader' CHECK (role IN ('admin', 'reader')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
