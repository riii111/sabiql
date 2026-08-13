SET NAMES utf8mb4;

USE sabiql_test;

CREATE TABLE mysql_cli_fixture (
    id INT PRIMARY KEY,
    nullable_text TEXT NULL,
    empty_text TEXT NOT NULL,
    unicode_text TEXT NOT NULL,
    json_value JSON NOT NULL,
    blob_value BLOB NOT NULL
) CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;

INSERT INTO mysql_cli_fixture (
    id,
    nullable_text,
    empty_text,
    unicode_text,
    json_value,
    blob_value
) VALUES (
    1,
    NULL,
    '',
    '日本語の値 🐬',
    '{"array":[1,true],"text":"空文字ではない"}',
    X'00FF10'
);

GRANT SYSTEM_VARIABLES_ADMIN ON *.* TO 'sabiql'@'%';
