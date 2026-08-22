SET NAMES utf8mb4;

USE sabiql_test;

-- ==========================================
-- DEMO SCHEMA
-- ==========================================

CREATE TABLE demo_customers (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    email VARCHAR(255) NOT NULL,
    display_name VARCHAR(100) NOT NULL,
    status ENUM('trial', 'active', 'suspended', 'closed') NOT NULL DEFAULT 'trial',
    preferences JSON NOT NULL,
    email_domain VARCHAR(100)
        GENERATED ALWAYS AS (SUBSTRING_INDEX(email, '@', -1)) STORED,
    internal_note VARCHAR(255) INVISIBLE,
    created_at DATETIME(6) NOT NULL,
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uq_demo_customers_email (email),
    KEY idx_demo_customers_status_domain (status, email_domain)
) ENGINE=InnoDB CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;

CREATE TABLE demo_categories (
    id INT UNSIGNED NOT NULL AUTO_INCREMENT,
    parent_id INT UNSIGNED NULL,
    slug VARCHAR(80) NOT NULL,
    name VARCHAR(100) NOT NULL,
    sort_order SMALLINT UNSIGNED NOT NULL DEFAULT 0,
    metadata JSON NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uq_demo_categories_slug (slug),
    KEY idx_demo_categories_parent (parent_id),
    CONSTRAINT fk_demo_categories_parent
        FOREIGN KEY (parent_id) REFERENCES demo_categories (id)
        ON UPDATE CASCADE ON DELETE SET NULL
) ENGINE=InnoDB CHARACTER SET utf8mb4;

CREATE TABLE demo_products (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    category_id INT UNSIGNED NOT NULL,
    sku VARCHAR(32) NOT NULL,
    name VARCHAR(160) NOT NULL,
    description TEXT NOT NULL,
    status ENUM('draft', 'active', 'archived') NOT NULL DEFAULT 'draft',
    tags SET('featured', 'cloud', 'security', 'analytics', 'mobile', 'open-source')
        NOT NULL DEFAULT '',
    price DECIMAL(10, 2) UNSIGNED NOT NULL,
    cost_price DECIMAL(10, 2) UNSIGNED NOT NULL,
    margin_amount DECIMAL(10, 2)
        GENERATED ALWAYS AS (price - cost_price) STORED,
    attributes JSON NOT NULL,
    released_on DATE NULL,
    created_at DATETIME NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uq_demo_products_sku (sku),
    KEY idx_demo_products_category_status (category_id, status),
    FULLTEXT KEY ft_demo_products_search (name, description),
    CONSTRAINT chk_demo_products_price CHECK (price >= cost_price),
    CONSTRAINT fk_demo_products_category
        FOREIGN KEY (category_id) REFERENCES demo_categories (id)
        ON UPDATE CASCADE ON DELETE RESTRICT
) ENGINE=InnoDB CHARACTER SET utf8mb4;

CREATE TABLE demo_warehouses (
    id SMALLINT UNSIGNED NOT NULL AUTO_INCREMENT,
    code CHAR(6) NOT NULL,
    name VARCHAR(100) NOT NULL,
    timezone VARCHAR(50) NOT NULL,
    location POINT NOT NULL SRID 4326,
    capacity INT UNSIGNED NOT NULL,
    active TINYINT(1) NOT NULL DEFAULT 1,
    PRIMARY KEY (id),
    UNIQUE KEY uq_demo_warehouses_code (code),
    SPATIAL KEY sp_demo_warehouses_location (location),
    CONSTRAINT chk_demo_warehouses_capacity CHECK (capacity > 0)
) ENGINE=InnoDB CHARACTER SET utf8mb4;

CREATE TABLE demo_inventory (
    warehouse_id SMALLINT UNSIGNED NOT NULL,
    product_id BIGINT UNSIGNED NOT NULL,
    quantity INT UNSIGNED NOT NULL,
    reserved_quantity INT UNSIGNED NOT NULL DEFAULT 0,
    available_quantity INT
        GENERATED ALWAYS AS (quantity - reserved_quantity) STORED,
    reorder_point INT UNSIGNED NOT NULL DEFAULT 10,
    last_counted_at DATETIME NULL,
    PRIMARY KEY (warehouse_id, product_id),
    KEY idx_demo_inventory_product (product_id),
    KEY idx_demo_inventory_available (available_quantity),
    CONSTRAINT chk_demo_inventory_reserved CHECK (reserved_quantity <= quantity),
    CONSTRAINT fk_demo_inventory_warehouse
        FOREIGN KEY (warehouse_id) REFERENCES demo_warehouses (id)
        ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT fk_demo_inventory_product
        FOREIGN KEY (product_id) REFERENCES demo_products (id)
        ON UPDATE CASCADE ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE demo_orders (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    order_number CHAR(12) NOT NULL,
    customer_id BIGINT UNSIGNED NOT NULL,
    warehouse_id SMALLINT UNSIGNED NOT NULL,
    status ENUM('cart', 'placed', 'paid', 'packed', 'shipped', 'delivered', 'cancelled')
        NOT NULL,
    currency CHAR(3) NOT NULL DEFAULT 'USD',
    subtotal DECIMAL(12, 2) UNSIGNED NOT NULL,
    tax_amount DECIMAL(12, 2) UNSIGNED NOT NULL DEFAULT 0,
    discount_amount DECIMAL(12, 2) UNSIGNED NOT NULL DEFAULT 0,
    shipping_amount DECIMAL(12, 2) UNSIGNED NOT NULL DEFAULT 0,
    grand_total DECIMAL(12, 2)
        GENERATED ALWAYS AS (subtotal + tax_amount + shipping_amount - discount_amount) STORED,
    shipping_address JSON NOT NULL,
    risk_score DECIMAL(5, 2) UNSIGNED INVISIBLE NOT NULL DEFAULT 0,
    placed_at DATETIME(6) NULL,
    created_at DATETIME(6) NOT NULL,
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
        ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    UNIQUE KEY uq_demo_orders_number (order_number),
    KEY idx_demo_orders_customer_created (customer_id, created_at DESC),
    KEY idx_demo_orders_status_created (status, created_at DESC),
    CONSTRAINT chk_demo_orders_discount CHECK (discount_amount <= subtotal + tax_amount),
    CONSTRAINT fk_demo_orders_customer
        FOREIGN KEY (customer_id) REFERENCES demo_customers (id)
        ON UPDATE CASCADE ON DELETE RESTRICT,
    CONSTRAINT fk_demo_orders_warehouse
        FOREIGN KEY (warehouse_id) REFERENCES demo_warehouses (id)
        ON UPDATE CASCADE ON DELETE RESTRICT
) ENGINE=InnoDB CHARACTER SET utf8mb4;

CREATE TABLE demo_order_items (
    order_id BIGINT UNSIGNED NOT NULL,
    line_number SMALLINT UNSIGNED NOT NULL,
    product_id BIGINT UNSIGNED NOT NULL,
    quantity SMALLINT UNSIGNED NOT NULL,
    unit_price DECIMAL(10, 2) UNSIGNED NOT NULL,
    discount_amount DECIMAL(10, 2) UNSIGNED NOT NULL DEFAULT 0,
    line_total DECIMAL(12, 2)
        GENERATED ALWAYS AS (quantity * unit_price - discount_amount) STORED,
    configuration JSON NULL,
    PRIMARY KEY (order_id, line_number),
    KEY idx_demo_order_items_product (product_id),
    CONSTRAINT chk_demo_order_items_quantity CHECK (quantity > 0),
    CONSTRAINT fk_demo_order_items_order
        FOREIGN KEY (order_id) REFERENCES demo_orders (id)
        ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT fk_demo_order_items_product
        FOREIGN KEY (product_id) REFERENCES demo_products (id)
        ON UPDATE CASCADE ON DELETE RESTRICT
) ENGINE=InnoDB;

CREATE TABLE demo_payments (
    id BINARY(16) NOT NULL,
    order_id BIGINT UNSIGNED NOT NULL,
    method ENUM('card', 'bank_transfer', 'wallet', 'invoice') NOT NULL,
    status ENUM('pending', 'authorized', 'captured', 'failed', 'refunded') NOT NULL,
    amount DECIMAL(12, 2) UNSIGNED NOT NULL,
    provider_response JSON NULL,
    processed_at DATETIME(6) NULL,
    PRIMARY KEY (id),
    UNIQUE KEY uq_demo_payments_order (order_id),
    KEY idx_demo_payments_status_processed (status, processed_at),
    CONSTRAINT fk_demo_payments_order
        FOREIGN KEY (order_id) REFERENCES demo_orders (id)
        ON UPDATE CASCADE ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE demo_product_reviews (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    product_id BIGINT UNSIGNED NOT NULL,
    customer_id BIGINT UNSIGNED NOT NULL,
    rating TINYINT UNSIGNED NOT NULL,
    title VARCHAR(160) NOT NULL,
    body TEXT NOT NULL,
    labels SET('verified', 'helpful', 'detailed', 'early-access') NOT NULL DEFAULT '',
    moderation JSON NOT NULL,
    created_at DATETIME NOT NULL,
    PRIMARY KEY (id),
    KEY idx_demo_reviews_product_rating (product_id, rating),
    KEY idx_demo_reviews_customer (customer_id),
    FULLTEXT KEY ft_demo_reviews_content (title, body),
    CONSTRAINT chk_demo_reviews_rating CHECK (rating BETWEEN 1 AND 5),
    CONSTRAINT fk_demo_reviews_product
        FOREIGN KEY (product_id) REFERENCES demo_products (id)
        ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT fk_demo_reviews_customer
        FOREIGN KEY (customer_id) REFERENCES demo_customers (id)
        ON UPDATE CASCADE ON DELETE CASCADE
) ENGINE=InnoDB CHARACTER SET utf8mb4;

CREATE TABLE demo_order_status_history (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    order_id BIGINT UNSIGNED NOT NULL,
    old_status VARCHAR(20) NULL,
    new_status VARCHAR(20) NOT NULL,
    changed_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    KEY idx_demo_order_history_order_changed (order_id, changed_at),
    CONSTRAINT fk_demo_order_history_order
        FOREIGN KEY (order_id) REFERENCES demo_orders (id)
        ON UPDATE CASCADE ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE demo_analytics_events (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    occurred_at DATETIME NOT NULL,
    customer_id BIGINT UNSIGNED NULL,
    session_id CHAR(36) NOT NULL,
    properties JSON NOT NULL,
    event_name VARCHAR(50)
        GENERATED ALWAYS AS (JSON_UNQUOTE(JSON_EXTRACT(properties, '$.event'))) STORED,
    source ENUM('web', 'mobile', 'api', 'worker') NOT NULL,
    PRIMARY KEY (id, occurred_at),
    KEY idx_demo_events_name_time (event_name, occurred_at DESC),
    KEY idx_demo_events_customer_time (customer_id, occurred_at DESC)
) ENGINE=InnoDB
PARTITION BY RANGE (YEAR(occurred_at)) (
    PARTITION p2025 VALUES LESS THAN (2026),
    PARTITION p2026 VALUES LESS THAN (2027),
    PARTITION pmax VALUES LESS THAN MAXVALUE
);

CREATE TRIGGER demo_order_item_reduce_stock
AFTER INSERT ON demo_order_items
FOR EACH ROW
UPDATE demo_inventory
SET quantity = quantity - NEW.quantity
WHERE warehouse_id = (SELECT warehouse_id FROM demo_orders WHERE id = NEW.order_id)
  AND product_id = NEW.product_id;

CREATE TRIGGER demo_order_status_audit
AFTER UPDATE ON demo_orders
FOR EACH ROW
INSERT INTO demo_order_status_history (order_id, old_status, new_status)
SELECT NEW.id, OLD.status, NEW.status
WHERE NOT (OLD.status <=> NEW.status);

-- ==========================================
-- DEMO DATA
-- ==========================================

INSERT INTO demo_customers (
    email, display_name, status, preferences, internal_note, created_at
) VALUES
('aiko@example.jp', 'Aiko Tanaka', 'active', JSON_OBJECT('locale', 'ja-JP', 'theme', 'tokyo-night', 'alerts', JSON_ARRAY('email', 'push')), 'high-value', '2025-01-05 09:15:00.100000'),
('ben@example.com', 'Ben Carter', 'active', JSON_OBJECT('locale', 'en-US', 'theme', 'catppuccin', 'alerts', JSON_ARRAY('email')), NULL, '2025-01-12 14:20:00.200000'),
('carla@example.es', 'Carla Ruiz', 'trial', JSON_OBJECT('locale', 'es-ES', 'theme', 'dracula', 'alerts', JSON_ARRAY()), 'onboarding', '2025-02-03 08:30:00.300000'),
('dev@example.dev', 'Dev Patel', 'active', JSON_OBJECT('locale', 'en-GB', 'theme', 'gruvbox', 'alerts', JSON_ARRAY('push')), NULL, '2025-02-18 16:45:00.400000'),
('elena@example.it', 'Elena Rossi', 'suspended', JSON_OBJECT('locale', 'it-IT', 'theme', 'nord', 'alerts', JSON_ARRAY('email')), 'payment review', '2025-03-02 11:10:00.500000'),
('fatima@example.ma', 'Fatima Zahra', 'active', JSON_OBJECT('locale', 'fr-FR', 'theme', 'tokyo-night', 'alerts', JSON_ARRAY('email', 'sms')), NULL, '2025-03-19 19:05:00.600000'),
('grace@example.ca', 'Grace Lee', 'active', JSON_OBJECT('locale', 'en-CA', 'theme', 'catppuccin', 'alerts', JSON_ARRAY('push')), NULL, '2025-04-01 07:55:00.700000'),
('hugo@example.fr', 'Hugo Martin', 'closed', JSON_OBJECT('locale', 'fr-FR', 'theme', 'light', 'alerts', JSON_ARRAY()), 'requested deletion', '2025-04-22 13:35:00.800000'),
('ines@example.pt', 'Ines Silva', 'trial', JSON_OBJECT('locale', 'pt-PT', 'theme', 'nord', 'alerts', JSON_ARRAY('email')), NULL, '2025-05-08 10:40:00.900000'),
('joon@example.kr', 'Joon Park', 'active', JSON_OBJECT('locale', 'ko-KR', 'theme', 'tokyo-night', 'alerts', JSON_ARRAY('email', 'push')), 'beta tester', '2025-05-27 21:15:00.010000'),
('kofi@example.gh', 'Kofi Mensah', 'active', JSON_OBJECT('locale', 'en-GH', 'theme', 'gruvbox', 'alerts', JSON_ARRAY('sms')), NULL, '2025-06-11 06:25:00.020000'),
('lucia@example.ar', 'Lucia Gomez', 'active', JSON_OBJECT('locale', 'es-AR', 'theme', 'dracula', 'alerts', JSON_ARRAY('email')), NULL, '2025-06-30 18:50:00.030000'),
('marta@example.pl', 'Marta Nowak', 'trial', JSON_OBJECT('locale', 'pl-PL', 'theme', 'light', 'alerts', JSON_ARRAY('push')), 'education plan', '2025-07-14 12:00:00.040000'),
('noah@example.nz', 'Noah Wilson', 'active', JSON_OBJECT('locale', 'en-NZ', 'theme', 'catppuccin', 'alerts', JSON_ARRAY('email', 'push')), NULL, '2025-08-09 04:45:00.050000'),
('olga@example.ua', 'Olga Kovalenko', 'suspended', JSON_OBJECT('locale', 'uk-UA', 'theme', 'nord', 'alerts', JSON_ARRAY('email')), 'identity review', '2025-09-17 15:35:00.060000'),
('pablo@example.mx', 'Pablo Diaz', 'active', JSON_OBJECT('locale', 'es-MX', 'theme', 'gruvbox', 'alerts', JSON_ARRAY('email', 'sms')), NULL, '2025-10-04 17:20:00.070000');

INSERT INTO demo_categories (parent_id, slug, name, sort_order, metadata) VALUES
(NULL, 'infrastructure', 'Infrastructure', 10, JSON_OBJECT('icon', 'server', 'color', '#3b82f6')),
(1, 'databases', 'Databases', 11, JSON_OBJECT('icon', 'database', 'color', '#2563eb')),
(1, 'observability', 'Observability', 12, JSON_OBJECT('icon', 'activity', 'color', '#0ea5e9')),
(NULL, 'developer-tools', 'Developer Tools', 20, JSON_OBJECT('icon', 'terminal', 'color', '#8b5cf6')),
(4, 'editors', 'Editors', 21, JSON_OBJECT('icon', 'code', 'color', '#7c3aed')),
(4, 'automation', 'Automation', 22, JSON_OBJECT('icon', 'workflow', 'color', '#a855f7')),
(NULL, 'security', 'Security', 30, JSON_OBJECT('icon', 'shield', 'color', '#ef4444')),
(7, 'identity', 'Identity', 31, JSON_OBJECT('icon', 'key', 'color', '#dc2626')),
(NULL, 'analytics', 'Analytics', 40, JSON_OBJECT('icon', 'chart', 'color', '#10b981'));

INSERT INTO demo_products (
    category_id, sku, name, description, status, tags, price, cost_price, attributes,
    released_on, created_at
) VALUES
(2, 'DB-001', 'Cloud Ledger', 'Managed transactional database with automatic backups.', 'active', 'featured,cloud', 249.00, 92.00, JSON_OBJECT('regions', JSON_ARRAY('nrt', 'fra', 'iad'), 'ha', true, 'storage_gb', 250), '2025-01-15', '2024-11-01 09:00:00'),
(2, 'DB-002', 'Vector Vault', 'Vector search storage for retrieval workloads.', 'active', 'cloud,analytics', 189.00, 71.00, JSON_OBJECT('dimensions', 3072, 'indexes', JSON_ARRAY('hnsw', 'flat')), '2025-02-01', '2024-12-12 10:00:00'),
(2, 'DB-003', 'Edge SQLite Sync', 'Offline-first synchronization for embedded databases.', 'active', 'mobile,open-source', 79.00, 21.00, JSON_OBJECT('conflict_strategy', 'crdt', 'max_devices', 50), '2025-02-20', '2025-01-03 11:00:00'),
(3, 'OBS-001', 'Trace Explorer', 'Distributed tracing and latency analysis.', 'active', 'featured,cloud,analytics', 159.00, 58.00, JSON_OBJECT('retention_days', 30, 'sampling', 0.25), '2025-03-10', '2025-01-22 12:00:00'),
(3, 'OBS-002', 'Log Harbor', 'Structured log ingestion with archive policies.', 'active', 'cloud,analytics', 129.00, 43.00, JSON_OBJECT('ingest_gb', 500, 'formats', JSON_ARRAY('json', 'text')), '2025-03-18', '2025-02-05 13:00:00'),
(3, 'OBS-003', 'Metric Pulse', 'Time-series dashboards and anomaly alerts.', 'active', 'analytics', 99.00, 30.00, JSON_OBJECT('cardinality', 'high', 'alert_channels', JSON_ARRAY('email', 'slack')), '2025-04-02', '2025-02-18 14:00:00'),
(5, 'ED-001', 'Terminal Studio', 'Fast keyboard-driven development environment.', 'active', 'featured,open-source', 49.00, 12.00, JSON_OBJECT('platforms', JSON_ARRAY('macOS', 'Linux', 'Windows'), 'themes', 24), '2025-04-11', '2025-02-28 15:00:00'),
(5, 'ED-002', 'Schema Canvas', 'Visual schema editor with migration previews.', 'active', 'analytics', 119.00, 36.00, JSON_OBJECT('dialects', JSON_ARRAY('PostgreSQL', 'MySQL', 'SQLite'), 'collaboration', true), '2025-04-25', '2025-03-07 16:00:00'),
(5, 'ED-003', 'JSON Workbench', 'Tree editor and semantic diff for JSON documents.', 'active', 'open-source', 39.00, 9.00, JSON_OBJECT('max_document_mb', 50, 'schema_validation', true), '2025-05-02', '2025-03-19 17:00:00'),
(6, 'AUTO-001', 'Deploy Relay', 'Repeatable deployment workflows with approvals.', 'active', 'cloud,security', 179.00, 64.00, JSON_OBJECT('runners', 10, 'environments', JSON_ARRAY('dev', 'staging', 'prod')), '2025-05-14', '2025-03-30 18:00:00'),
(6, 'AUTO-002', 'Data Pipeline Kit', 'Composable batch and streaming workflows.', 'active', 'cloud,analytics', 219.00, 84.00, JSON_OBJECT('connectors', 42, 'streaming', true), '2025-05-29', '2025-04-09 19:00:00'),
(6, 'AUTO-003', 'Runbook Robot', 'Operations automation for recurring incidents.', 'draft', 'security', 139.00, 47.00, JSON_OBJECT('actions', JSON_ARRAY('http', 'shell', 'sql'), 'audit_log', true), NULL, '2025-04-21 20:00:00'),
(7, 'SEC-001', 'Secret Sentinel', 'Secret scanning for repositories and build logs.', 'active', 'featured,security', 149.00, 52.00, JSON_OBJECT('patterns', 320, 'custom_rules', true), '2025-06-03', '2025-04-30 21:00:00'),
(7, 'SEC-002', 'Dependency Radar', 'Dependency risk and advisory monitoring.', 'active', 'security,open-source', 89.00, 24.00, JSON_OBJECT('ecosystems', JSON_ARRAY('cargo', 'npm', 'pip'), 'sbom', true), '2025-06-17', '2025-05-12 22:00:00'),
(8, 'ID-001', 'Passkey Gateway', 'Passwordless authentication and device policies.', 'active', 'featured,security', 199.00, 73.00, JSON_OBJECT('protocols', JSON_ARRAY('WebAuthn', 'OIDC'), 'risk_engine', true), '2025-07-01', '2025-05-23 09:30:00'),
(8, 'ID-002', 'Team Directory', 'Identity lifecycle and role synchronization.', 'active', 'cloud,security', 169.00, 61.00, JSON_OBJECT('provisioning', 'SCIM', 'directory_sources', 8), '2025-07-15', '2025-06-02 10:30:00'),
(9, 'AN-001', 'Funnel Lab', 'Product funnel exploration and cohort reports.', 'active', 'featured,analytics', 139.00, 44.00, JSON_OBJECT('retention_months', 24, 'realtime', true), '2025-07-28', '2025-06-11 11:30:00'),
(9, 'AN-002', 'Revenue Lens', 'Revenue metrics, forecasts, and variance analysis.', 'active', 'cloud,analytics', 229.00, 88.00, JSON_OBJECT('currencies', JSON_ARRAY('USD', 'EUR', 'JPY'), 'forecast_models', 4), '2025-08-09', '2025-06-22 12:30:00'),
(9, 'AN-003', 'Experiment Board', 'A/B test analysis with statistical guardrails.', 'active', 'analytics', 109.00, 34.00, JSON_OBJECT('methods', JSON_ARRAY('frequentist', 'bayesian'), 'segments', 100), '2025-08-21', '2025-07-03 13:30:00'),
(1, 'INF-001', 'Private Network', 'Isolated service networking and egress controls.', 'active', 'cloud,security', 299.00, 118.00, JSON_OBJECT('cidr_blocks', 8, 'nat_gateways', 2), '2025-09-02', '2025-07-14 14:30:00'),
(1, 'INF-002', 'Object Archive', 'Durable object storage with lifecycle policies.', 'active', 'cloud', 69.00, 18.00, JSON_OBJECT('storage_tb', 10, 'tiers', JSON_ARRAY('hot', 'cold')), '2025-09-16', '2025-07-25 15:30:00'),
(4, 'DEV-001', 'API Inspector', 'HTTP and event API exploration for teams.', 'active', 'open-source', 59.00, 15.00, JSON_OBJECT('protocols', JSON_ARRAY('HTTP', 'WebSocket', 'SSE'), 'collections', true), '2025-10-01', '2025-08-04 16:30:00'),
(4, 'DEV-002', 'Release Notes Bot', 'Release note generation from reviewed changes.', 'draft', 'open-source', 29.00, 7.00, JSON_OBJECT('providers', JSON_ARRAY('GitHub', 'GitLab'), 'templates', 12), NULL, '2025-08-15 17:30:00'),
(7, 'SEC-003', 'Policy Engine', 'Policy evaluation for deployment and data access.', 'archived', 'security,open-source', 129.00, 41.00, JSON_OBJECT('language', 'Rego', 'bundles', true), '2024-06-01', '2024-01-15 18:30:00');

INSERT INTO demo_warehouses (code, name, timezone, location, capacity) VALUES
('TYO001', 'Tokyo Fulfillment', 'Asia/Tokyo', ST_SRID(POINT(139.6917, 35.6895), 4326), 20000),
('BER001', 'Berlin Fulfillment', 'Europe/Berlin', ST_SRID(POINT(13.4050, 52.5200), 4326), 12000),
('SFO001', 'San Francisco Hub', 'America/Los_Angeles', ST_SRID(POINT(-122.4194, 37.7749), 4326), 18000),
('SYD001', 'Sydney Hub', 'Australia/Sydney', ST_SRID(POINT(151.2093, -33.8688), 4326), 9000);

INSERT INTO demo_inventory (
    warehouse_id, product_id, quantity, reserved_quantity, reorder_point, last_counted_at
)
SELECT
    warehouse.id,
    product.id,
    80 + MOD(product.id * 7 + warehouse.id * 11, 120),
    MOD(product.id + warehouse.id, 12),
    20 + MOD(product.id, 15),
    TIMESTAMPADD(DAY, -MOD(product.id * warehouse.id, 30), '2026-02-15 08:00:00')
FROM demo_warehouses AS warehouse
CROSS JOIN demo_products AS product;

INSERT INTO demo_orders (
    order_number, customer_id, warehouse_id, status, currency, subtotal, tax_amount,
    discount_amount, shipping_amount, shipping_address, risk_score, placed_at, created_at
)
WITH RECURSIVE sequence (n) AS (
    SELECT 1
    UNION ALL
    SELECT n + 1 FROM sequence WHERE n < 40
)
SELECT
    CONCAT('ORD-', LPAD(n, 8, '0')),
    MOD(n - 1, 16) + 1,
    MOD(n - 1, 4) + 1,
    ELT(MOD(n - 1, 7) + 1, 'placed', 'paid', 'packed', 'shipped', 'delivered', 'cancelled', 'cart'),
    ELT(MOD(n - 1, 3) + 1, 'USD', 'EUR', 'JPY'),
    80.00 + MOD(n * 47, 900),
    ROUND((80.00 + MOD(n * 47, 900)) * 0.10, 2),
    IF(MOD(n, 5) = 0, 25.00, 0.00),
    IF(MOD(n, 4) = 0, 0.00, 12.50),
    JSON_OBJECT(
        'recipient', CONCAT('Customer ', MOD(n - 1, 16) + 1),
        'city', ELT(MOD(n - 1, 4) + 1, 'Tokyo', 'Berlin', 'San Francisco', 'Sydney'),
        'postal_code', CONCAT('ZIP-', LPAD(n, 4, '0')),
        'lines', JSON_ARRAY(CONCAT(n, ' Demo Street'), 'Suite 8')
    ),
    MOD(n * 13, 100),
    IF(MOD(n - 1, 7) = 6, NULL, TIMESTAMPADD(DAY, -n, '2026-02-15 12:00:00')),
    TIMESTAMPADD(DAY, -n, '2026-02-15 12:00:00')
FROM sequence;

INSERT INTO demo_order_status_history (order_id, old_status, new_status, changed_at)
SELECT id, NULL, status, created_at
FROM demo_orders;

INSERT INTO demo_order_items (
    order_id, line_number, product_id, quantity, unit_price, discount_amount, configuration
)
SELECT
    orders.id,
    line.line_number,
    product.id,
    MOD(orders.id + line.line_number, 3) + 1,
    product.price,
    IF(MOD(orders.id + line.line_number, 7) = 0, 10.00, 0.00),
    JSON_OBJECT(
        'plan', ELT(MOD(orders.id + line.line_number, 3) + 1, 'starter', 'team', 'enterprise'),
        'seats', 5 + MOD(orders.id * line.line_number, 45),
        'auto_renew', MOD(orders.id, 2) = 0
    )
FROM demo_orders AS orders
CROSS JOIN (
    SELECT 1 AS line_number
    UNION ALL
    SELECT 2
) AS line
JOIN demo_products AS product
    ON product.id = MOD(orders.id * 3 + line.line_number - 1, 24) + 1;

INSERT INTO demo_payments (
    id, order_id, method, status, amount, provider_response, processed_at
)
SELECT
    UUID_TO_BIN(CONCAT('00000000-0000-0000-0000-', LPAD(id, 12, '0'))),
    id,
    ELT(MOD(id - 1, 4) + 1, 'card', 'bank_transfer', 'wallet', 'invoice'),
    CASE status
        WHEN 'cart' THEN 'pending'
        WHEN 'cancelled' THEN 'failed'
        WHEN 'placed' THEN 'authorized'
        ELSE 'captured'
    END,
    grand_total,
    JSON_OBJECT(
        'provider', ELT(MOD(id - 1, 3) + 1, 'atlas-pay', 'north-bank', 'demo-wallet'),
        'reference', CONCAT('PAY-', LPAD(id, 8, '0')),
        'attempts', MOD(id, 3) + 1
    ),
    IF(status IN ('cart', 'cancelled'), NULL, TIMESTAMPADD(MINUTE, 3, placed_at))
FROM demo_orders;

INSERT INTO demo_product_reviews (
    product_id, customer_id, rating, title, body, labels, moderation, created_at
)
WITH RECURSIVE sequence (n) AS (
    SELECT 1
    UNION ALL
    SELECT n + 1 FROM sequence WHERE n < 60
)
SELECT
    MOD(n * 5 - 1, 24) + 1,
    MOD(n * 7 - 1, 16) + 1,
    MOD(n - 1, 5) + 1,
    CONCAT(ELT(MOD(n - 1, 5) + 1, 'Needs work', 'Useful', 'Solid choice', 'Excellent', 'Essential'), ' #', n),
    CONCAT(
        'Used this product for ', MOD(n, 12) + 1,
        ' months. Setup was ',
        ELT(MOD(n - 1, 4) + 1, 'straightforward', 'well documented', 'a little involved', 'very quick'),
        ' and the team response was helpful.'
    ),
    ELT(MOD(n - 1, 4) + 1, 'verified', 'verified,helpful', 'detailed', 'verified,early-access'),
    JSON_OBJECT('state', IF(MOD(n, 13) = 0, 'flagged', 'approved'), 'automated', true, 'score', MOD(n * 17, 100) / 100),
    TIMESTAMPADD(DAY, -n * 2, '2026-02-15 16:00:00')
FROM sequence;

INSERT INTO demo_analytics_events (
    occurred_at, customer_id, session_id, properties, source
)
WITH RECURSIVE sequence (n) AS (
    SELECT 1
    UNION ALL
    SELECT n + 1 FROM sequence WHERE n < 250
)
SELECT
    TIMESTAMPADD(HOUR, -n * 3, '2026-02-15 18:00:00'),
    IF(MOD(n, 11) = 0, NULL, MOD(n - 1, 16) + 1),
    CONCAT('00000000-0000-0000-0001-', LPAD(MOD(n - 1, 50) + 1, 12, '0')),
    JSON_OBJECT(
        'event', ELT(MOD(n - 1, 6) + 1, 'page_view', 'search', 'product_opened', 'checkout_started', 'purchase', 'export'),
        'path', ELT(MOD(n - 1, 5) + 1, '/', '/catalog', '/orders', '/analytics', '/settings'),
        'duration_ms', MOD(n * 137, 5000),
        'experiment', IF(MOD(n, 3) = 0, JSON_OBJECT('name', 'compact-grid', 'variant', 'B'), NULL)
    ),
    ELT(MOD(n - 1, 4) + 1, 'web', 'mobile', 'api', 'worker')
FROM sequence;

-- ==========================================
-- DEMO VIEWS
-- ==========================================

CREATE VIEW demo_order_summary AS
SELECT
    orders.id,
    orders.order_number,
    customers.display_name AS customer,
    warehouses.code AS warehouse,
    orders.status,
    COUNT(items.line_number) AS item_count,
    SUM(items.quantity) AS unit_count,
    orders.currency,
    orders.grand_total,
    orders.created_at
FROM demo_orders AS orders
JOIN demo_customers AS customers ON customers.id = orders.customer_id
JOIN demo_warehouses AS warehouses ON warehouses.id = orders.warehouse_id
JOIN demo_order_items AS items ON items.order_id = orders.id
GROUP BY
    orders.id,
    orders.order_number,
    customers.display_name,
    warehouses.code,
    orders.status,
    orders.currency,
    orders.grand_total,
    orders.created_at;

CREATE VIEW demo_inventory_status AS
SELECT
    warehouses.code AS warehouse,
    products.sku,
    products.name AS product,
    inventory.quantity,
    inventory.reserved_quantity,
    inventory.available_quantity,
    inventory.reorder_point,
    inventory.available_quantity <= inventory.reorder_point AS needs_reorder
FROM demo_inventory AS inventory
JOIN demo_warehouses AS warehouses ON warehouses.id = inventory.warehouse_id
JOIN demo_products AS products ON products.id = inventory.product_id;

CREATE VIEW demo_daily_event_counts AS
SELECT
    DATE(occurred_at) AS event_date,
    event_name,
    source,
    COUNT(*) AS event_count,
    COUNT(DISTINCT customer_id) AS unique_customers
FROM demo_analytics_events
GROUP BY DATE(occurred_at), event_name, source;
