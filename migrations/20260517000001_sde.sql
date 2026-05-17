-- SDE subset: categories, groups, market groups, types, and a meta row.

CREATE TABLE eve_categories (
    category_id BIGINT PRIMARY KEY,
    name        TEXT NOT NULL,
    published   BOOLEAN NOT NULL
);

CREATE TABLE eve_groups (
    group_id    BIGINT PRIMARY KEY,
    name        TEXT NOT NULL,
    category_id BIGINT NOT NULL REFERENCES eve_categories(category_id),
    published   BOOLEAN NOT NULL
);

CREATE TABLE eve_market_groups (
    market_group_id BIGINT PRIMARY KEY,
    name            TEXT NOT NULL,
    parent_id       BIGINT REFERENCES eve_market_groups(market_group_id)
);

CREATE TABLE eve_types (
    type_id          BIGINT PRIMARY KEY,
    name             TEXT NOT NULL,
    group_id         BIGINT NOT NULL REFERENCES eve_groups(group_id),
    market_group_id  BIGINT REFERENCES eve_market_groups(market_group_id),
    volume           DOUBLE PRECISION NOT NULL,
    packaged_volume  DOUBLE PRECISION NOT NULL,
    published        BOOLEAN NOT NULL
);

CREATE INDEX eve_types_market_group_idx ON eve_types(market_group_id) WHERE market_group_id IS NOT NULL;
CREATE INDEX eve_types_group_idx ON eve_types(group_id);

CREATE TABLE eve_sde_meta (
    id        INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    version   TEXT NOT NULL,
    loaded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Helper for parsing Fuzzwork's 1/0/empty boolean fields out of staged TEXT
-- columns. Used only by the SDE load path.
CREATE OR REPLACE FUNCTION to_bool(t TEXT) RETURNS BOOLEAN
LANGUAGE SQL IMMUTABLE AS $$
SELECT CASE
    WHEN t IS NULL OR t = '' THEN FALSE
    WHEN t IN ('1','t','true','True','TRUE','y','yes','Y') THEN TRUE
    ELSE FALSE
END
$$;
