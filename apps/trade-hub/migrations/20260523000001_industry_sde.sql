-- Industry SDE tables: blueprints, activities, materials, products, PI schematics.
-- Loaded by eve-industry from Fuzzwork CSVs.

CREATE TABLE sde_blueprints (
    blueprint_type_id   BIGINT PRIMARY KEY,
    max_production_limit INT NOT NULL
);

CREATE TABLE sde_blueprint_activities (
    blueprint_type_id   BIGINT NOT NULL REFERENCES sde_blueprints(blueprint_type_id),
    activity_id         INT NOT NULL,
    time_secs           INT NOT NULL,
    PRIMARY KEY (blueprint_type_id, activity_id)
);

CREATE TABLE sde_blueprint_materials (
    blueprint_type_id   BIGINT NOT NULL,
    activity_id         INT NOT NULL,
    material_type_id    BIGINT NOT NULL,
    quantity            INT NOT NULL,
    PRIMARY KEY (blueprint_type_id, activity_id, material_type_id),
    FOREIGN KEY (blueprint_type_id, activity_id)
        REFERENCES sde_blueprint_activities(blueprint_type_id, activity_id)
);

CREATE TABLE sde_blueprint_products (
    blueprint_type_id   BIGINT NOT NULL,
    activity_id         INT NOT NULL,
    product_type_id     BIGINT NOT NULL,
    quantity            INT NOT NULL,
    PRIMARY KEY (blueprint_type_id, activity_id, product_type_id),
    FOREIGN KEY (blueprint_type_id, activity_id)
        REFERENCES sde_blueprint_activities(blueprint_type_id, activity_id)
);

-- PI schematics
CREATE TABLE sde_planet_schematics (
    schematic_id        INT PRIMARY KEY,
    schematic_name      TEXT NOT NULL,
    cycle_time_secs     INT NOT NULL
);

CREATE TABLE sde_planet_schematic_types (
    schematic_id        INT NOT NULL REFERENCES sde_planet_schematics(schematic_id),
    type_id             BIGINT NOT NULL,
    quantity            INT NOT NULL,
    is_input            BOOLEAN NOT NULL,
    PRIMARY KEY (schematic_id, type_id)
);

-- Index: find the blueprint that produces a given type_id
CREATE INDEX sde_blueprint_products_product_idx
    ON sde_blueprint_products(product_type_id);

-- Index: find the PI schematic that produces a given type_id
CREATE INDEX sde_planet_schematic_types_output_idx
    ON sde_planet_schematic_types(type_id) WHERE is_input = false;
