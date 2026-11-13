CREATE TABLE tasks (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
    due_date TEXT CHECK (due_date IS NULL OR date(due_date) = due_date),
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'completed')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX tasks_due_date_status ON tasks (due_date, status);
