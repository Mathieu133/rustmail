-- Never used before: recreated so that expires_at can be NULL (permanent block).
DROP TABLE IF EXISTS "blocked_users";

CREATE TABLE "blocked_users" (
    "user_id" TEXT NOT NULL PRIMARY KEY,
    "user_name" TEXT NOT NULL,
    "blocked_by" TEXT NOT NULL,
    "blocked_at" INTEGER NOT NULL,
    "expires_at" INTEGER,
    "reason" TEXT
);
