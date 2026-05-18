PRAGMA foreign_keys = ON;

DROP VIEW IF EXISTS daily_sales_report;
DROP VIEW IF EXISTS category_tree;
DROP VIEW IF EXISTS user_activity_summary;
DROP VIEW IF EXISTS product_catalog;
DROP VIEW IF EXISTS order_summary;

DROP TABLE IF EXISTS settings;
DROP TABLE IF EXISTS agent_memory_items;
DROP TABLE IF EXISTS agent_tool_calls;
DROP TABLE IF EXISTS agent_messages;
DROP TABLE IF EXISTS agent_threads;
DROP TABLE IF EXISTS audit_logs;
DROP TABLE IF EXISTS daily_metrics;
DROP TABLE IF EXISTS analytics_events;
DROP TABLE IF EXISTS notification_preferences;
DROP TABLE IF EXISTS notifications;
DROP TABLE IF EXISTS review_votes;
DROP TABLE IF EXISTS reviews;
DROP TABLE IF EXISTS promotions;
DROP TABLE IF EXISTS campaigns;
DROP TABLE IF EXISTS coupon_usages;
DROP TABLE IF EXISTS coupons;
DROP TABLE IF EXISTS shipping_rates;
DROP TABLE IF EXISTS shipping_zones;
DROP TABLE IF EXISTS payments;
DROP TABLE IF EXISTS order_items;
DROP TABLE IF EXISTS orders;
DROP TABLE IF EXISTS stock_levels;
DROP TABLE IF EXISTS inventory_movements;
DROP TABLE IF EXISTS warehouses;
DROP TABLE IF EXISTS user_favorites;
DROP TABLE IF EXISTS product_tags;
DROP TABLE IF EXISTS products;
DROP TABLE IF EXISTS tags;
DROP TABLE IF EXISTS categories;
DROP TABLE IF EXISTS departments;
DROP TABLE IF EXISTS organization_members;
DROP TABLE IF EXISTS organizations;
DROP TABLE IF EXISTS password_reset_tokens;
DROP TABLE IF EXISTS api_keys;
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS users;

CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    full_name TEXT,
    phone TEXT,
    avatar_url TEXT,
    bio TEXT,
    location TEXT,
    company TEXT,
    job_title TEXT,
    website TEXT,
    github_url TEXT,
    twitter_handle TEXT,
    timezone TEXT,
    language_preference TEXT DEFAULT 'en',
    password_hash TEXT NOT NULL,
    email_verified INTEGER NOT NULL DEFAULT 0 CHECK (email_verified IN (0, 1)),
    phone_verified INTEGER NOT NULL DEFAULT 0 CHECK (phone_verified IN (0, 1)),
    two_factor_enabled INTEGER NOT NULL DEFAULT 0 CHECK (two_factor_enabled IN (0, 1)),
    last_login_at TEXT,
    failed_login_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT,
    account_status TEXT NOT NULL DEFAULT 'active',
    manager_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TEXT
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_account_status ON users(account_status);
CREATE INDEX idx_users_created_at ON users(created_at DESC);
CREATE INDEX idx_users_manager_id ON users(manager_id);

CREATE TRIGGER users_updated_at
AFTER UPDATE ON users
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE users SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    ip_address TEXT,
    user_agent TEXT,
    device_type TEXT,
    last_activity_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);

CREATE TABLE api_keys (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT '[]',
    rate_limit INTEGER NOT NULL DEFAULT 1000,
    last_used_at TEXT,
    expires_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_api_keys_key_prefix ON api_keys(key_prefix);

CREATE TABLE password_reset_tokens (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    used_at TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_password_reset_tokens_user_id ON password_reset_tokens(user_id);
CREATE INDEX idx_password_reset_tokens_expires_at ON password_reset_tokens(expires_at);

CREATE TABLE organizations (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    logo_url TEXT,
    banner_url TEXT,
    website TEXT,
    email TEXT,
    phone TEXT,
    country TEXT,
    state_province TEXT,
    city TEXT,
    postal_code TEXT,
    street_address TEXT,
    business_type TEXT,
    industry TEXT,
    employee_count TEXT,
    founded_year INTEGER,
    registration_number TEXT,
    tax_id TEXT,
    verified INTEGER NOT NULL DEFAULT 0 CHECK (verified IN (0, 1)),
    subscription_tier TEXT NOT NULL DEFAULT 'free',
    subscription_status TEXT NOT NULL DEFAULT 'active',
    billing_email TEXT,
    next_billing_date TEXT,
    parent_organization_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL,
    owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_organizations_slug ON organizations(slug);
CREATE INDEX idx_organizations_owner_id ON organizations(owner_id);
CREATE INDEX idx_organizations_subscription_tier ON organizations(subscription_tier);
CREATE INDEX idx_organizations_parent_id ON organizations(parent_organization_id);

CREATE TRIGGER organizations_updated_at
AFTER UPDATE ON organizations
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE organizations SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

CREATE TABLE organization_members (
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member',
    permissions TEXT NOT NULL DEFAULT '[]',
    invited_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    joined_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (organization_id, user_id)
);

CREATE INDEX idx_org_members_user_id ON organization_members(user_id);
CREATE INDEX idx_org_members_role ON organization_members(role);

CREATE TABLE departments (
    id INTEGER PRIMARY KEY,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    code TEXT,
    description TEXT,
    parent_department_id INTEGER REFERENCES departments(id) ON DELETE SET NULL,
    manager_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    budget REAL,
    headcount INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (organization_id, code)
);

CREATE INDEX idx_departments_organization_id ON departments(organization_id);
CREATE INDEX idx_departments_parent_id ON departments(parent_department_id);

CREATE TABLE categories (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    icon TEXT,
    image_url TEXT,
    parent_id INTEGER REFERENCES categories(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    product_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_categories_parent_id ON categories(parent_id);
CREATE INDEX idx_categories_slug ON categories(slug);
CREATE INDEX idx_categories_is_active ON categories(is_active);

CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#6B7280',
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tags_slug ON tags(slug);
CREATE INDEX idx_tags_usage_count ON tags(usage_count DESC);

CREATE TABLE products (
    id INTEGER PRIMARY KEY,
    sku TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    price REAL NOT NULL CHECK (price >= 0),
    cost_price REAL,
    discount_price REAL,
    discount_percentage REAL,
    tax_rate REAL,
    stock_quantity INTEGER NOT NULL DEFAULT 0,
    reorder_point INTEGER NOT NULL DEFAULT 10,
    weight REAL,
    color TEXT,
    size TEXT,
    material TEXT,
    image_url TEXT,
    supplier_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    is_featured INTEGER NOT NULL DEFAULT 0 CHECK (is_featured IN (0, 1)),
    rating REAL,
    review_count INTEGER NOT NULL DEFAULT 0,
    metadata TEXT NOT NULL DEFAULT '{}',
    search_label TEXT GENERATED ALWAYS AS (lower(name || ' ' || sku)) VIRTUAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_products_sku ON products(sku);
CREATE INDEX idx_products_organization_id ON products(organization_id);
CREATE INDEX idx_products_category_id ON products(category_id);
CREATE INDEX idx_products_is_active ON products(is_active);
CREATE INDEX idx_products_search_label ON products(search_label);

CREATE TABLE product_tags (
    product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (product_id, tag_id)
);

CREATE INDEX idx_product_tags_tag_id ON product_tags(tag_id);

CREATE TABLE user_favorites (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, product_id)
);

CREATE INDEX idx_user_favorites_product_id ON user_favorites(product_id);

CREATE TABLE warehouses (
    id INTEGER PRIMARY KEY,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    address TEXT,
    city TEXT,
    country TEXT,
    latitude REAL,
    longitude REAL,
    capacity INTEGER,
    manager_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (organization_id, code)
);

CREATE INDEX idx_warehouses_organization_id ON warehouses(organization_id);

CREATE TABLE inventory_movements (
    id INTEGER PRIMARY KEY,
    warehouse_id INTEGER NOT NULL REFERENCES warehouses(id) ON DELETE CASCADE,
    product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    movement_type TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    reference_type TEXT,
    reference_id INTEGER,
    notes TEXT,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_inventory_movements_warehouse_id ON inventory_movements(warehouse_id);
CREATE INDEX idx_inventory_movements_product_id ON inventory_movements(product_id);
CREATE INDEX idx_inventory_movements_created_at ON inventory_movements(created_at DESC);

CREATE TABLE stock_levels (
    warehouse_id INTEGER NOT NULL REFERENCES warehouses(id) ON DELETE CASCADE,
    product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    quantity INTEGER NOT NULL DEFAULT 0,
    reserved_quantity INTEGER NOT NULL DEFAULT 0,
    reorder_point INTEGER NOT NULL DEFAULT 10,
    last_counted_at TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (warehouse_id, product_id),
    CHECK (quantity >= 0),
    CHECK (reserved_quantity >= 0)
) WITHOUT ROWID;

CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    order_number TEXT NOT NULL UNIQUE,
    user_id INTEGER REFERENCES users(id) ON DELETE RESTRICT,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending',
    payment_status TEXT NOT NULL DEFAULT 'pending',
    fulfillment_status TEXT NOT NULL DEFAULT 'unshipped',
    total_amount REAL NOT NULL,
    subtotal REAL NOT NULL,
    tax_amount REAL NOT NULL DEFAULT 0,
    shipping_amount REAL NOT NULL DEFAULT 0,
    discount_amount REAL NOT NULL DEFAULT 0,
    coupon_id INTEGER,
    customer_email TEXT,
    shipping_city TEXT,
    shipping_country TEXT,
    tracking_number TEXT,
    carrier TEXT,
    warehouse_id INTEGER REFERENCES warehouses(id) ON DELETE SET NULL,
    shipped_at TEXT,
    delivered_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_orders_order_number ON orders(order_number);
CREATE INDEX idx_orders_user_id ON orders(user_id);
CREATE INDEX idx_orders_organization_id ON orders(organization_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created_at ON orders(created_at DESC);

CREATE TABLE order_items (
    id INTEGER PRIMARY KEY,
    order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE RESTRICT,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price REAL NOT NULL,
    discount_price REAL,
    line_total REAL NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_order_items_order_id ON order_items(order_id);
CREATE INDEX idx_order_items_product_id ON order_items(product_id);

CREATE TRIGGER order_items_stock_update
AFTER INSERT ON order_items
FOR EACH ROW
BEGIN
    UPDATE products
       SET stock_quantity = stock_quantity - NEW.quantity
     WHERE id = NEW.product_id;
END;

CREATE TABLE payments (
    id INTEGER PRIMARY KEY,
    order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    amount REAL NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',
    payment_method TEXT NOT NULL,
    payment_status TEXT NOT NULL DEFAULT 'pending',
    transaction_id TEXT UNIQUE,
    risk_score REAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_payments_order_id ON payments(order_id);
CREATE INDEX idx_payments_payment_status ON payments(payment_status);

CREATE TABLE shipping_zones (
    id INTEGER PRIMARY KEY,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    countries TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE shipping_rates (
    id INTEGER PRIMARY KEY,
    zone_id INTEGER NOT NULL REFERENCES shipping_zones(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    min_weight REAL,
    max_weight REAL,
    min_order_amount REAL,
    max_order_amount REAL,
    rate REAL NOT NULL,
    estimated_days_min INTEGER,
    estimated_days_max INTEGER,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_shipping_rates_zone_id ON shipping_rates(zone_id);

CREATE TABLE coupons (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    description TEXT,
    discount_type TEXT NOT NULL,
    discount_value REAL NOT NULL,
    min_order_amount REAL,
    max_discount_amount REAL,
    usage_limit INTEGER,
    usage_count INTEGER NOT NULL DEFAULT 0,
    starts_at TEXT,
    expires_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_coupons_code ON coupons(code);
CREATE INDEX idx_coupons_organization_id ON coupons(organization_id);
CREATE INDEX idx_coupons_is_active ON coupons(is_active);

CREATE TABLE coupon_usages (
    id INTEGER PRIMARY KEY,
    coupon_id INTEGER NOT NULL REFERENCES coupons(id) ON DELETE CASCADE,
    order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    discount_applied REAL NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE campaigns (
    id INTEGER PRIMARY KEY,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    campaign_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    budget REAL,
    spent REAL NOT NULL DEFAULT 0,
    starts_at TEXT,
    ends_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_campaigns_organization_id ON campaigns(organization_id);
CREATE INDEX idx_campaigns_status ON campaigns(status);

CREATE TABLE promotions (
    id INTEGER PRIMARY KEY,
    campaign_id INTEGER REFERENCES campaigns(id) ON DELETE SET NULL,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    product_id INTEGER REFERENCES products(id) ON DELETE CASCADE,
    promotion_type TEXT NOT NULL,
    discount_value REAL,
    starts_at TEXT,
    ends_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE reviews (
    id INTEGER PRIMARY KEY,
    product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    order_id INTEGER REFERENCES orders(id) ON DELETE SET NULL,
    rating INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 5),
    title TEXT,
    content TEXT,
    pros TEXT,
    cons TEXT,
    is_verified_purchase INTEGER NOT NULL DEFAULT 0 CHECK (is_verified_purchase IN (0, 1)),
    helpful_count INTEGER NOT NULL DEFAULT 0,
    report_count INTEGER NOT NULL DEFAULT 0,
    moderation_status TEXT NOT NULL DEFAULT 'pending',
    moderated_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    moderated_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_reviews_product_id ON reviews(product_id);
CREATE INDEX idx_reviews_user_id ON reviews(user_id);
CREATE INDEX idx_reviews_rating ON reviews(rating);
CREATE INDEX idx_reviews_status ON reviews(moderation_status);

CREATE TABLE review_votes (
    review_id INTEGER NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    is_helpful INTEGER NOT NULL CHECK (is_helpful IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (review_id, user_id)
);

CREATE TABLE notifications (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    data TEXT,
    is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1)),
    read_at TEXT,
    action_url TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_notifications_user_id ON notifications(user_id);
CREATE INDEX idx_notifications_is_read ON notifications(is_read);
CREATE INDEX idx_notifications_created_at ON notifications(created_at DESC);

CREATE TABLE notification_preferences (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type TEXT NOT NULL,
    email_enabled INTEGER NOT NULL DEFAULT 1 CHECK (email_enabled IN (0, 1)),
    push_enabled INTEGER NOT NULL DEFAULT 1 CHECK (push_enabled IN (0, 1)),
    sms_enabled INTEGER NOT NULL DEFAULT 0 CHECK (sms_enabled IN (0, 1)),
    in_app_enabled INTEGER NOT NULL DEFAULT 1 CHECK (in_app_enabled IN (0, 1)),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, notification_type)
);

CREATE TABLE analytics_events (
    id INTEGER PRIMARY KEY,
    event_type TEXT NOT NULL,
    user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    session_id TEXT,
    organization_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL,
    properties TEXT NOT NULL DEFAULT '{}',
    page_url TEXT,
    referrer_url TEXT,
    ip_address TEXT,
    country TEXT,
    city TEXT,
    device_type TEXT,
    browser TEXT,
    os TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_analytics_events_event_type ON analytics_events(event_type);
CREATE INDEX idx_analytics_events_user_id ON analytics_events(user_id);
CREATE INDEX idx_analytics_events_session_id ON analytics_events(session_id);
CREATE INDEX idx_analytics_events_created_at ON analytics_events(created_at DESC);

CREATE TABLE daily_metrics (
    id INTEGER PRIMARY KEY,
    organization_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    metric_date TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    dimensions TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (organization_id, metric_date, metric_name, dimensions)
);

CREATE INDEX idx_daily_metrics_organization_id ON daily_metrics(organization_id);
CREATE INDEX idx_daily_metrics_date ON daily_metrics(metric_date DESC);
CREATE INDEX idx_daily_metrics_name ON daily_metrics(metric_name);

CREATE TABLE audit_logs (
    id INTEGER PRIMARY KEY,
    user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    organization_id INTEGER REFERENCES organizations(id) ON DELETE SET NULL,
    table_name TEXT NOT NULL,
    record_id TEXT,
    action TEXT NOT NULL,
    old_values TEXT,
    new_values TEXT,
    changed_fields TEXT,
    ip_address TEXT,
    user_agent TEXT,
    request_id TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_organization_id ON audit_logs(organization_id);
CREATE INDEX idx_audit_logs_table_name ON audit_logs(table_name);
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at DESC);
CREATE INDEX idx_audit_logs_request_id ON audit_logs(request_id);

CREATE TABLE agent_threads (
    id INTEGER PRIMARY KEY,
    external_thread_id TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    model TEXT NOT NULL,
    goal TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_agent_threads_status ON agent_threads(status);
CREATE INDEX idx_agent_threads_created_at ON agent_threads(created_at DESC);

CREATE TABLE agent_messages (
    id INTEGER PRIMARY KEY,
    thread_id INTEGER NOT NULL REFERENCES agent_threads(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('system', 'user', 'assistant', 'tool')),
    turn_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    token_count INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (thread_id, turn_index, role)
);

CREATE INDEX idx_agent_messages_thread_id ON agent_messages(thread_id);
CREATE INDEX idx_agent_messages_role ON agent_messages(role);
CREATE INDEX idx_agent_messages_created_at ON agent_messages(created_at DESC);

CREATE TABLE agent_tool_calls (
    id INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES agent_messages(id) ON DELETE CASCADE,
    tool_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    result_text TEXT,
    status TEXT NOT NULL DEFAULT 'ok',
    elapsed_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_agent_tool_calls_message_id ON agent_tool_calls(message_id);
CREATE INDEX idx_agent_tool_calls_tool_name ON agent_tool_calls(tool_name);

CREATE TABLE agent_memory_items (
    id INTEGER PRIMARY KEY,
    thread_id INTEGER REFERENCES agent_threads(id) ON DELETE SET NULL,
    memory_key TEXT NOT NULL,
    memory_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    body TEXT NOT NULL,
    embedding_model TEXT,
    importance REAL NOT NULL DEFAULT 0,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_agent_memory_items_key ON agent_memory_items(memory_key);
CREATE INDEX idx_agent_memory_items_type ON agent_memory_items(memory_type);
CREATE INDEX idx_agent_memory_items_importance ON agent_memory_items(importance DESC);

CREATE TABLE settings (
    id INTEGER PRIMARY KEY,
    organization_id INTEGER REFERENCES organizations(id) ON DELETE CASCADE,
    setting_key TEXT NOT NULL,
    setting_value TEXT,
    setting_type TEXT NOT NULL DEFAULT 'string',
    is_public INTEGER NOT NULL DEFAULT 0 CHECK (is_public IN (0, 1)),
    is_encrypted INTEGER NOT NULL DEFAULT 0 CHECK (is_encrypted IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (organization_id, setting_key)
);

CREATE INDEX idx_settings_organization_id ON settings(organization_id);

CREATE VIEW order_summary AS
SELECT
    o.id,
    o.order_number,
    o.status,
    o.payment_status,
    o.fulfillment_status,
    o.total_amount,
    o.created_at,
    u.username,
    u.email,
    org.name AS organization_name,
    COUNT(oi.id) AS item_count,
    SUM(oi.quantity) AS total_quantity
FROM orders o
LEFT JOIN users u ON o.user_id = u.id
JOIN organizations org ON o.organization_id = org.id
LEFT JOIN order_items oi ON o.id = oi.order_id
GROUP BY o.id;

CREATE VIEW product_catalog AS
SELECT
    p.id,
    p.sku,
    p.name,
    p.description,
    c.name AS category_name,
    c.slug AS category_slug,
    p.price,
    p.discount_price,
    p.stock_quantity,
    p.is_active,
    p.is_featured,
    p.rating,
    p.review_count,
    org.name AS organization_name,
    group_concat(DISTINCT t.name) AS tags
FROM products p
LEFT JOIN categories c ON p.category_id = c.id
LEFT JOIN organizations org ON p.organization_id = org.id
LEFT JOIN product_tags pt ON p.id = pt.product_id
LEFT JOIN tags t ON pt.tag_id = t.id
GROUP BY p.id;

CREATE VIEW user_activity_summary AS
SELECT
    u.id,
    u.username,
    u.email,
    u.account_status,
    COUNT(DISTINCT om.organization_id) AS organization_count,
    COUNT(DISTINCT o.id) AS order_count,
    COALESCE(SUM(o.total_amount), 0) AS total_spent,
    COUNT(DISTINCT r.id) AS review_count,
    COUNT(DISTINCT uf.product_id) AS favorite_count,
    u.last_login_at,
    u.created_at
FROM users u
LEFT JOIN organization_members om ON u.id = om.user_id
LEFT JOIN orders o ON u.id = o.user_id AND o.status = 'completed'
LEFT JOIN reviews r ON u.id = r.user_id
LEFT JOIN user_favorites uf ON u.id = uf.user_id
GROUP BY u.id;

CREATE VIEW category_tree AS
WITH RECURSIVE category_path(id, slug, name, parent_id, depth, full_path, sort_path) AS (
    SELECT id, slug, name, parent_id, 1, name, printf('%04d', sort_order)
    FROM categories
    WHERE parent_id IS NULL
    UNION ALL
    SELECT c.id, c.slug, c.name, c.parent_id, cp.depth + 1,
           cp.full_path || ' > ' || c.name,
           cp.sort_path || '.' || printf('%04d', c.sort_order)
    FROM categories c
    JOIN category_path cp ON c.parent_id = cp.id
)
SELECT id, slug, name, parent_id, depth, full_path
FROM category_path
ORDER BY sort_path;

CREATE VIEW daily_sales_report AS
SELECT
    date(o.created_at) AS sale_date,
    o.organization_id,
    org.name AS organization_name,
    COUNT(DISTINCT o.id) AS order_count,
    SUM(o.total_amount) AS total_revenue,
    AVG(o.total_amount) AS avg_order_value,
    SUM(o.discount_amount) AS total_discounts,
    COUNT(DISTINCT o.user_id) AS unique_customers
FROM orders o
JOIN organizations org ON o.organization_id = org.id
WHERE o.status NOT IN ('cancelled', 'refunded')
GROUP BY date(o.created_at), o.organization_id, org.name;
