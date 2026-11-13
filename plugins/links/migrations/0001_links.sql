CREATE TABLE item_links (
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (source_kind, source_id, target_kind, target_id),
    CHECK (source_kind != target_kind OR source_id != target_id)
);

CREATE INDEX item_links_target ON item_links (target_kind, target_id);

CREATE TABLE external_references (
    owner_kind TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    reference_kind TEXT NOT NULL CHECK (reference_kind IN ('file', 'url')),
    value TEXT NOT NULL CHECK (length(value) BETWEEN 1 AND 2048),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (owner_kind, owner_id, reference_kind, value)
);
