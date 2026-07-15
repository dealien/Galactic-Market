ALTER TABLE diplomatic_relations
ADD COLUMN neutral_since_tick BIGINT NOT NULL DEFAULT 0;
