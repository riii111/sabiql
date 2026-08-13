SET NAMES utf8mb4;

USE sabiql_test;

CREATE TABLE mysql_cli_fixture (
    id INT PRIMARY KEY,
    nullable_text TEXT NULL,
    empty_text TEXT NOT NULL,
    unicode_text TEXT NOT NULL,
    json_value JSON NOT NULL,
    blob_value BLOB NOT NULL,
    invisible_value INT INVISIBLE,
    generated_value INT GENERATED ALWAYS AS (id * 2) STORED,
    unsigned_value BIGINT UNSIGNED NOT NULL,
    precise_decimal DECIMAL(65, 30) NOT NULL,
    scientific_value DOUBLE NOT NULL
) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;

INSERT INTO mysql_cli_fixture (
    id,
    nullable_text,
    empty_text,
    unicode_text,
    json_value,
    blob_value,
    unsigned_value,
    precise_decimal,
    scientific_value
) VALUES (
    1,
    NULL,
    '',
    '日本語の値 🐬',
    '{"array":[1,true],"text":"空文字ではない"}',
    X'00FF10',
    18446744073709551615,
    12345678901234567890123456789012345.123456789012345678901234567890,
    1.23e100
);

CREATE TABLE mysql_preview_composite (
    first_key INT NOT NULL,
    second_key INT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (second_key, first_key)
) CHARACTER SET utf8mb4;

INSERT INTO mysql_preview_composite (first_key, second_key, payload)
VALUES (1, 20, 'first'), (2, 10, 'second');

CREATE TABLE mysql_preview_no_pk (
    duplicate_value VARCHAR(20) NOT NULL,
    payload TEXT NOT NULL
) CHARACTER SET utf8mb4;

INSERT INTO mysql_preview_no_pk (duplicate_value, payload)
VALUES ('same', 'first'), ('same', 'second');

CREATE TABLE mysql_preview_empty (
    id INT PRIMARY KEY,
    payload TEXT
) CHARACTER SET utf8mb4;

CREATE VIEW mysql_preview_view AS
SELECT id, unicode_text FROM mysql_cli_fixture;

GRANT SYSTEM_VARIABLES_ADMIN ON *.* TO 'sabiql'@'%';
