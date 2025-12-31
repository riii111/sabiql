CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ==========================================
-- SCHEMAS
-- ==========================================
CREATE SCHEMA IF NOT EXISTS sales;
CREATE SCHEMA IF NOT EXISTS marketing;
CREATE SCHEMA IF NOT EXISTS analytics;

-- ==========================================
-- FUNCTIONS (for triggers)
-- ==========================================
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION update_stock_on_order()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE products SET stock_quantity = stock_quantity - NEW.quantity
        WHERE id = NEW.product_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE products SET stock_quantity = stock_quantity + OLD.quantity
        WHERE id = OLD.product_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ==========================================
-- 1. users (public schema)
-- ==========================================
CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    username VARCHAR(50) NOT NULL UNIQUE,
    email VARCHAR(100) NOT NULL UNIQUE,
    full_name VARCHAR(100),
    phone VARCHAR(20),
    avatar_url TEXT,
    bio TEXT,
    location VARCHAR(100),
    company VARCHAR(100),
    job_title VARCHAR(100),
    website VARCHAR(255),
    github_url VARCHAR(255),
    twitter_handle VARCHAR(50),
    timezone VARCHAR(50),
    language_preference VARCHAR(10),
    password_hash VARCHAR(255) NOT NULL,
    email_verified BOOLEAN DEFAULT FALSE,
    phone_verified BOOLEAN DEFAULT FALSE,
    two_factor_enabled BOOLEAN DEFAULT FALSE,
    last_login_at TIMESTAMP WITH TIME ZONE,
    failed_login_attempts INT DEFAULT 0,
    locked_until TIMESTAMP WITH TIME ZONE,
    account_status VARCHAR(20) DEFAULT 'active',
    manager_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_account_status ON users(account_status);
CREATE INDEX idx_users_created_at ON users(created_at DESC);
CREATE INDEX idx_users_manager_id ON users(manager_id);

CREATE TRIGGER users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 2. sessions (authentication)
-- ==========================================
CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    ip_address INET,
    user_agent TEXT,
    device_type VARCHAR(50),
    last_activity_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);

-- ==========================================
-- 3. api_keys (authentication)
-- ==========================================
CREATE TABLE api_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    key_hash VARCHAR(255) NOT NULL UNIQUE,
    key_prefix VARCHAR(10) NOT NULL,
    scopes TEXT[] DEFAULT '{}',
    rate_limit INT DEFAULT 1000,
    last_used_at TIMESTAMP WITH TIME ZONE,
    expires_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_api_keys_key_prefix ON api_keys(key_prefix);

-- ==========================================
-- 4. password_reset_tokens
-- ==========================================
CREATE TABLE password_reset_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    used_at TIMESTAMP WITH TIME ZONE,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_password_reset_tokens_user_id ON password_reset_tokens(user_id);
CREATE INDEX idx_password_reset_tokens_expires_at ON password_reset_tokens(expires_at);

-- ==========================================
-- 5. organizations
-- ==========================================
CREATE TABLE organizations (
    id BIGSERIAL PRIMARY KEY,
    slug VARCHAR(100) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    logo_url TEXT,
    banner_url TEXT,
    website VARCHAR(255),
    email VARCHAR(100),
    phone VARCHAR(20),
    country VARCHAR(100),
    state_province VARCHAR(100),
    city VARCHAR(100),
    postal_code VARCHAR(20),
    street_address VARCHAR(255),
    business_type VARCHAR(50),
    industry VARCHAR(100),
    employee_count VARCHAR(50),
    founded_year INT,
    registration_number VARCHAR(100),
    tax_id VARCHAR(100),
    verified BOOLEAN DEFAULT FALSE,
    subscription_tier VARCHAR(50) DEFAULT 'free',
    subscription_status VARCHAR(20) DEFAULT 'active',
    billing_email VARCHAR(100),
    next_billing_date DATE,
    parent_organization_id BIGINT REFERENCES organizations(id) ON DELETE SET NULL,
    owner_id BIGINT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_organizations_slug ON organizations(slug);
CREATE INDEX idx_organizations_owner_id ON organizations(owner_id);
CREATE INDEX idx_organizations_subscription_tier ON organizations(subscription_tier);
CREATE INDEX idx_organizations_parent_id ON organizations(parent_organization_id);

CREATE TRIGGER organizations_updated_at
    BEFORE UPDATE ON organizations
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 6. organization_members (many-to-many)
-- ==========================================
CREATE TABLE organization_members (
    organization_id BIGINT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL DEFAULT 'member',
    permissions TEXT[] DEFAULT '{}',
    invited_by BIGINT REFERENCES users(id) ON DELETE SET NULL,
    joined_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (organization_id, user_id)
);

CREATE INDEX idx_org_members_user_id ON organization_members(user_id);
CREATE INDEX idx_org_members_role ON organization_members(role);

-- ==========================================
-- 7. departments (self-referencing)
-- ==========================================
CREATE TABLE departments (
    id BIGSERIAL PRIMARY KEY,
    organization_id BIGINT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    code VARCHAR(20),
    description TEXT,
    parent_department_id BIGINT REFERENCES departments(id) ON DELETE SET NULL,
    manager_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    budget DECIMAL(15, 2),
    headcount INT DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(organization_id, code)
);

CREATE INDEX idx_departments_organization_id ON departments(organization_id);
CREATE INDEX idx_departments_parent_id ON departments(parent_department_id);

CREATE TRIGGER departments_updated_at
    BEFORE UPDATE ON departments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 8. categories (self-referencing hierarchy)
-- ==========================================
CREATE TABLE categories (
    id BIGSERIAL PRIMARY KEY,
    slug VARCHAR(100) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    icon VARCHAR(50),
    image_url TEXT,
    parent_id BIGINT REFERENCES categories(id) ON DELETE CASCADE,
    sort_order INT DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE,
    product_count INT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_categories_parent_id ON categories(parent_id);
CREATE INDEX idx_categories_slug ON categories(slug);
CREATE INDEX idx_categories_is_active ON categories(is_active);

CREATE TRIGGER categories_updated_at
    BEFORE UPDATE ON categories
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 9. tags
-- ==========================================
CREATE TABLE tags (
    id BIGSERIAL PRIMARY KEY,
    slug VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(50) NOT NULL,
    color VARCHAR(7) DEFAULT '#6B7280',
    usage_count INT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tags_slug ON tags(slug);
CREATE INDEX idx_tags_usage_count ON tags(usage_count DESC);

-- ==========================================
-- 10. products
-- ==========================================
CREATE TABLE products (
    id BIGSERIAL PRIMARY KEY,
    sku VARCHAR(100) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    long_description TEXT,
    category_id BIGINT REFERENCES categories(id) ON DELETE SET NULL,
    price DECIMAL(10, 2) NOT NULL,
    cost_price DECIMAL(10, 2),
    currency VARCHAR(3) DEFAULT 'USD',
    discount_price DECIMAL(10, 2),
    discount_percentage DECIMAL(5, 2),
    tax_rate DECIMAL(5, 2),
    stock_quantity INT DEFAULT 0,
    reorder_level INT DEFAULT 10,
    weight DECIMAL(10, 3),
    weight_unit VARCHAR(10),
    dimensions_length DECIMAL(10, 3),
    dimensions_width DECIMAL(10, 3),
    dimensions_height DECIMAL(10, 3),
    dimension_unit VARCHAR(10),
    color VARCHAR(50),
    size VARCHAR(50),
    material VARCHAR(100),
    barcode VARCHAR(100),
    image_url TEXT,
    thumbnail_url TEXT,
    supplier_id BIGINT,
    organization_id BIGINT NOT NULL,
    is_active BOOLEAN DEFAULT TRUE,
    is_featured BOOLEAN DEFAULT FALSE,
    rating DECIMAL(3, 2),
    review_count INT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE
);

CREATE INDEX idx_products_sku ON products(sku);
CREATE INDEX idx_products_organization_id ON products(organization_id);
CREATE INDEX idx_products_category_id ON products(category_id);
CREATE INDEX idx_products_is_active ON products(is_active);

CREATE TRIGGER products_updated_at
    BEFORE UPDATE ON products
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 11. product_tags (many-to-many, composite PK)
-- ==========================================
CREATE TABLE product_tags (
    product_id BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    tag_id BIGINT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (product_id, tag_id)
);

CREATE INDEX idx_product_tags_tag_id ON product_tags(tag_id);

-- ==========================================
-- 12. user_favorites (many-to-many, composite PK)
-- ==========================================
CREATE TABLE user_favorites (
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    product_id BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    notes TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, product_id)
);

CREATE INDEX idx_user_favorites_product_id ON user_favorites(product_id);

-- ==========================================
-- 13. warehouses
-- ==========================================
CREATE TABLE warehouses (
    id BIGSERIAL PRIMARY KEY,
    organization_id BIGINT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    code VARCHAR(20) NOT NULL,
    name VARCHAR(100) NOT NULL,
    address TEXT,
    city VARCHAR(100),
    country VARCHAR(100),
    latitude DECIMAL(10, 8),
    longitude DECIMAL(11, 8),
    capacity INT,
    manager_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(organization_id, code)
);

CREATE INDEX idx_warehouses_organization_id ON warehouses(organization_id);

CREATE TRIGGER warehouses_updated_at
    BEFORE UPDATE ON warehouses
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 14. inventory_movements
-- ==========================================
CREATE TABLE inventory_movements (
    id BIGSERIAL PRIMARY KEY,
    warehouse_id BIGINT NOT NULL REFERENCES warehouses(id) ON DELETE CASCADE,
    product_id BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    movement_type VARCHAR(20) NOT NULL,
    quantity INT NOT NULL,
    reference_type VARCHAR(50),
    reference_id BIGINT,
    notes TEXT,
    performed_by BIGINT REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_inventory_movements_warehouse_id ON inventory_movements(warehouse_id);
CREATE INDEX idx_inventory_movements_product_id ON inventory_movements(product_id);
CREATE INDEX idx_inventory_movements_created_at ON inventory_movements(created_at DESC);

-- ==========================================
-- 15. stock_levels
-- ==========================================
CREATE TABLE stock_levels (
    warehouse_id BIGINT NOT NULL REFERENCES warehouses(id) ON DELETE CASCADE,
    product_id BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    quantity INT DEFAULT 0,
    reserved_quantity INT DEFAULT 0,
    reorder_point INT DEFAULT 10,
    last_counted_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (warehouse_id, product_id)
);

-- ==========================================
-- 16. sales.orders (in sales schema)
-- ==========================================
CREATE TABLE sales.orders (
    id BIGSERIAL PRIMARY KEY,
    order_number VARCHAR(50) NOT NULL UNIQUE,
    user_id BIGINT NOT NULL,
    organization_id BIGINT NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    payment_status VARCHAR(50) DEFAULT 'unpaid',
    shipping_status VARCHAR(50) DEFAULT 'unshipped',
    total_amount DECIMAL(12, 2) NOT NULL,
    subtotal DECIMAL(12, 2) NOT NULL,
    tax_amount DECIMAL(12, 2) DEFAULT 0,
    shipping_amount DECIMAL(12, 2) DEFAULT 0,
    discount_amount DECIMAL(12, 2) DEFAULT 0,
    currency VARCHAR(3) DEFAULT 'USD',
    coupon_id BIGINT,
    billing_first_name VARCHAR(100),
    billing_last_name VARCHAR(100),
    billing_email VARCHAR(100),
    billing_phone VARCHAR(20),
    billing_street_address VARCHAR(255),
    billing_city VARCHAR(100),
    billing_state_province VARCHAR(100),
    billing_postal_code VARCHAR(20),
    billing_country VARCHAR(100),
    shipping_first_name VARCHAR(100),
    shipping_last_name VARCHAR(100),
    shipping_phone VARCHAR(20),
    shipping_street_address VARCHAR(255),
    shipping_city VARCHAR(100),
    shipping_state_province VARCHAR(100),
    shipping_postal_code VARCHAR(20),
    shipping_country VARCHAR(100),
    shipping_method VARCHAR(100),
    tracking_number VARCHAR(100),
    carrier VARCHAR(100),
    warehouse_id BIGINT,
    estimated_delivery_date DATE,
    actual_delivery_date DATE,
    notes TEXT,
    internal_notes TEXT,
    customer_ip_address INET,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id) REFERENCES public.organizations(id) ON DELETE CASCADE,
    FOREIGN KEY (warehouse_id) REFERENCES public.warehouses(id) ON DELETE SET NULL
);

CREATE INDEX idx_sales_orders_order_number ON sales.orders(order_number);
CREATE INDEX idx_sales_orders_user_id ON sales.orders(user_id);
CREATE INDEX idx_sales_orders_organization_id ON sales.orders(organization_id);
CREATE INDEX idx_sales_orders_status ON sales.orders(status);
CREATE INDEX idx_sales_orders_created_at ON sales.orders(created_at DESC);

CREATE TRIGGER sales_orders_updated_at
    BEFORE UPDATE ON sales.orders
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 17. sales.order_items
-- ==========================================
CREATE TABLE sales.order_items (
    id BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES sales.orders(id) ON DELETE CASCADE,
    product_id BIGINT NOT NULL REFERENCES public.products(id) ON DELETE RESTRICT,
    quantity INT NOT NULL DEFAULT 1,
    unit_price DECIMAL(10, 2) NOT NULL,
    discount_price DECIMAL(10, 2),
    line_total DECIMAL(12, 2) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sales_order_items_order_id ON sales.order_items(order_id);
CREATE INDEX idx_sales_order_items_product_id ON sales.order_items(product_id);

CREATE TRIGGER order_items_stock_update
    AFTER INSERT OR DELETE ON sales.order_items
    FOR EACH ROW EXECUTE FUNCTION update_stock_on_order();

-- ==========================================
-- 18. sales.payments
-- ==========================================
CREATE TABLE sales.payments (
    id BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES sales.orders(id) ON DELETE CASCADE,
    amount DECIMAL(12, 2) NOT NULL,
    currency VARCHAR(3) DEFAULT 'USD',
    payment_method VARCHAR(50) NOT NULL,
    payment_status VARCHAR(50) DEFAULT 'pending',
    transaction_id VARCHAR(100) UNIQUE,
    gateway_response_code VARCHAR(50),
    authorization_code VARCHAR(100),
    cvv_verified BOOLEAN,
    risk_score DECIMAL(5, 2),
    notes TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sales_payments_order_id ON sales.payments(order_id);
CREATE INDEX idx_sales_payments_payment_status ON sales.payments(payment_status);

CREATE TRIGGER sales_payments_updated_at
    BEFORE UPDATE ON sales.payments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 19. sales.shipping_zones
-- ==========================================
CREATE TABLE sales.shipping_zones (
    id BIGSERIAL PRIMARY KEY,
    organization_id BIGINT NOT NULL REFERENCES public.organizations(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    countries TEXT[] NOT NULL,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_shipping_zones_organization_id ON sales.shipping_zones(organization_id);

-- ==========================================
-- 20. sales.shipping_rates
-- ==========================================
CREATE TABLE sales.shipping_rates (
    id BIGSERIAL PRIMARY KEY,
    zone_id BIGINT NOT NULL REFERENCES sales.shipping_zones(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    min_weight DECIMAL(10, 3),
    max_weight DECIMAL(10, 3),
    min_order_amount DECIMAL(12, 2),
    max_order_amount DECIMAL(12, 2),
    rate DECIMAL(10, 2) NOT NULL,
    estimated_days_min INT,
    estimated_days_max INT,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_shipping_rates_zone_id ON sales.shipping_rates(zone_id);

-- ==========================================
-- 21. marketing.coupons
-- ==========================================
CREATE TABLE marketing.coupons (
    id BIGSERIAL PRIMARY KEY,
    code VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    discount_type VARCHAR(20) NOT NULL,
    discount_value DECIMAL(10, 2) NOT NULL,
    min_order_amount DECIMAL(12, 2),
    max_discount_amount DECIMAL(12, 2),
    usage_limit INT,
    usage_count INT DEFAULT 0,
    usage_limit_per_user INT DEFAULT 1,
    applicable_product_ids BIGINT[],
    applicable_category_ids BIGINT[],
    starts_at TIMESTAMP WITH TIME ZONE,
    expires_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT TRUE,
    created_by BIGINT REFERENCES public.users(id) ON DELETE SET NULL,
    organization_id BIGINT NOT NULL REFERENCES public.organizations(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_coupons_code ON marketing.coupons(code);
CREATE INDEX idx_coupons_organization_id ON marketing.coupons(organization_id);
CREATE INDEX idx_coupons_is_active ON marketing.coupons(is_active);

CREATE TRIGGER marketing_coupons_updated_at
    BEFORE UPDATE ON marketing.coupons
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- Add FK from sales.orders to marketing.coupons
ALTER TABLE sales.orders ADD CONSTRAINT fk_orders_coupon
    FOREIGN KEY (coupon_id) REFERENCES marketing.coupons(id) ON DELETE SET NULL;

-- ==========================================
-- 22. marketing.coupon_usages
-- ==========================================
CREATE TABLE marketing.coupon_usages (
    id BIGSERIAL PRIMARY KEY,
    coupon_id BIGINT NOT NULL REFERENCES marketing.coupons(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    order_id BIGINT NOT NULL REFERENCES sales.orders(id) ON DELETE CASCADE,
    discount_applied DECIMAL(12, 2) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_coupon_usages_coupon_id ON marketing.coupon_usages(coupon_id);
CREATE INDEX idx_coupon_usages_user_id ON marketing.coupon_usages(user_id);

-- ==========================================
-- 23. marketing.campaigns
-- ==========================================
CREATE TABLE marketing.campaigns (
    id BIGSERIAL PRIMARY KEY,
    organization_id BIGINT NOT NULL REFERENCES public.organizations(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    campaign_type VARCHAR(50) NOT NULL,
    status VARCHAR(20) DEFAULT 'draft',
    budget DECIMAL(12, 2),
    spent DECIMAL(12, 2) DEFAULT 0,
    target_audience JSONB,
    starts_at TIMESTAMP WITH TIME ZONE,
    ends_at TIMESTAMP WITH TIME ZONE,
    created_by BIGINT REFERENCES public.users(id) ON DELETE SET NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_campaigns_organization_id ON marketing.campaigns(organization_id);
CREATE INDEX idx_campaigns_status ON marketing.campaigns(status);

CREATE TRIGGER marketing_campaigns_updated_at
    BEFORE UPDATE ON marketing.campaigns
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 24. marketing.promotions
-- ==========================================
CREATE TABLE marketing.promotions (
    id BIGSERIAL PRIMARY KEY,
    campaign_id BIGINT REFERENCES marketing.campaigns(id) ON DELETE CASCADE,
    organization_id BIGINT NOT NULL REFERENCES public.organizations(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    promotion_type VARCHAR(50) NOT NULL,
    discount_type VARCHAR(20),
    discount_value DECIMAL(10, 2),
    buy_quantity INT,
    get_quantity INT,
    applicable_products BIGINT[],
    starts_at TIMESTAMP WITH TIME ZONE,
    ends_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT TRUE,
    priority INT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_promotions_organization_id ON marketing.promotions(organization_id);
CREATE INDEX idx_promotions_campaign_id ON marketing.promotions(campaign_id);
CREATE INDEX idx_promotions_is_active ON marketing.promotions(is_active);

CREATE TRIGGER marketing_promotions_updated_at
    BEFORE UPDATE ON marketing.promotions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 25. reviews
-- ==========================================
CREATE TABLE reviews (
    id BIGSERIAL PRIMARY KEY,
    product_id BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    order_id BIGINT REFERENCES sales.orders(id) ON DELETE SET NULL,
    rating INT NOT NULL CHECK (rating >= 1 AND rating <= 5),
    title VARCHAR(200),
    content TEXT,
    pros TEXT,
    cons TEXT,
    is_verified_purchase BOOLEAN DEFAULT FALSE,
    helpful_count INT DEFAULT 0,
    not_helpful_count INT DEFAULT 0,
    status VARCHAR(20) DEFAULT 'pending',
    moderated_by BIGINT REFERENCES users(id) ON DELETE SET NULL,
    moderated_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_reviews_product_id ON reviews(product_id);
CREATE INDEX idx_reviews_user_id ON reviews(user_id);
CREATE INDEX idx_reviews_rating ON reviews(rating);
CREATE INDEX idx_reviews_status ON reviews(status);

CREATE TRIGGER reviews_updated_at
    BEFORE UPDATE ON reviews
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- 26. review_votes
-- ==========================================
CREATE TABLE review_votes (
    review_id BIGINT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    is_helpful BOOLEAN NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (review_id, user_id)
);

CREATE INDEX idx_review_votes_user_id ON review_votes(user_id);

-- ==========================================
-- 27. notifications
-- ==========================================
CREATE TABLE notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    type VARCHAR(50) NOT NULL,
    title VARCHAR(200) NOT NULL,
    message TEXT,
    data JSONB,
    is_read BOOLEAN DEFAULT FALSE,
    read_at TIMESTAMP WITH TIME ZONE,
    action_url TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_notifications_user_id ON notifications(user_id);
CREATE INDEX idx_notifications_is_read ON notifications(is_read);
CREATE INDEX idx_notifications_created_at ON notifications(created_at DESC);

-- ==========================================
-- 28. notification_preferences
-- ==========================================
CREATE TABLE notification_preferences (
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type VARCHAR(50) NOT NULL,
    email_enabled BOOLEAN DEFAULT TRUE,
    push_enabled BOOLEAN DEFAULT TRUE,
    sms_enabled BOOLEAN DEFAULT FALSE,
    in_app_enabled BOOLEAN DEFAULT TRUE,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, notification_type)
);

-- ==========================================
-- 29. analytics.events
-- ==========================================
CREATE TABLE analytics.events (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(100) NOT NULL,
    user_id BIGINT REFERENCES public.users(id) ON DELETE SET NULL,
    session_id UUID,
    organization_id BIGINT REFERENCES public.organizations(id) ON DELETE SET NULL,
    properties JSONB,
    page_url TEXT,
    referrer TEXT,
    user_agent TEXT,
    ip_address INET,
    country VARCHAR(2),
    city VARCHAR(100),
    device_type VARCHAR(20),
    browser VARCHAR(50),
    os VARCHAR(50),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_analytics_events_event_type ON analytics.events(event_type);
CREATE INDEX idx_analytics_events_user_id ON analytics.events(user_id);
CREATE INDEX idx_analytics_events_session_id ON analytics.events(session_id);
CREATE INDEX idx_analytics_events_created_at ON analytics.events(created_at DESC);
CREATE INDEX idx_analytics_events_properties ON analytics.events USING GIN(properties);

-- ==========================================
-- 30. analytics.daily_metrics
-- ==========================================
CREATE TABLE analytics.daily_metrics (
    id BIGSERIAL PRIMARY KEY,
    organization_id BIGINT NOT NULL REFERENCES public.organizations(id) ON DELETE CASCADE,
    metric_date DATE NOT NULL,
    metric_name VARCHAR(100) NOT NULL,
    metric_value DECIMAL(20, 4) NOT NULL,
    dimensions JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(organization_id, metric_date, metric_name, dimensions)
);

CREATE INDEX idx_daily_metrics_organization_id ON analytics.daily_metrics(organization_id);
CREATE INDEX idx_daily_metrics_date ON analytics.daily_metrics(metric_date DESC);
CREATE INDEX idx_daily_metrics_name ON analytics.daily_metrics(metric_name);

-- ==========================================
-- 31. audit_logs (enhanced)
-- ==========================================
CREATE TABLE audit_logs (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT,
    organization_id BIGINT,
    table_schema VARCHAR(50) DEFAULT 'public',
    table_name VARCHAR(100) NOT NULL,
    operation VARCHAR(20) NOT NULL,
    record_id BIGINT,
    old_values JSONB,
    new_values JSONB,
    changed_fields TEXT[],
    ip_address INET,
    user_agent TEXT,
    request_id UUID,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_organization_id ON audit_logs(organization_id);
CREATE INDEX idx_audit_logs_table_name ON audit_logs(table_name);
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at DESC);
CREATE INDEX idx_audit_logs_request_id ON audit_logs(request_id);

-- ==========================================
-- 32. settings
-- ==========================================
CREATE TABLE settings (
    id BIGSERIAL PRIMARY KEY,
    organization_id BIGINT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    setting_key VARCHAR(100) NOT NULL,
    setting_value TEXT NOT NULL,
    data_type VARCHAR(50),
    is_public BOOLEAN DEFAULT FALSE,
    is_encrypted BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(organization_id, setting_key)
);

CREATE INDEX idx_settings_organization_id ON settings(organization_id);

CREATE TRIGGER settings_updated_at
    BEFORE UPDATE ON settings
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ==========================================
-- VIEWS
-- ==========================================

-- Order summary view
CREATE VIEW sales.order_summary AS
SELECT
    o.id,
    o.order_number,
    u.username,
    u.email as user_email,
    org.name as organization_name,
    o.status,
    o.payment_status,
    o.shipping_status,
    COUNT(oi.id) as item_count,
    SUM(oi.quantity) as total_quantity,
    o.subtotal,
    o.discount_amount,
    o.tax_amount,
    o.shipping_amount,
    o.total_amount,
    o.currency,
    o.created_at,
    o.updated_at
FROM sales.orders o
JOIN public.users u ON o.user_id = u.id
JOIN public.organizations org ON o.organization_id = org.id
LEFT JOIN sales.order_items oi ON o.id = oi.order_id
GROUP BY o.id, u.username, u.email, org.name;

-- Product catalog view
CREATE VIEW product_catalog AS
SELECT
    p.id,
    p.sku,
    p.name,
    p.description,
    c.name as category_name,
    c.slug as category_slug,
    p.price,
    p.discount_price,
    p.stock_quantity,
    p.is_active,
    p.is_featured,
    p.rating,
    p.review_count,
    org.name as organization_name,
    array_agg(DISTINCT t.name) FILTER (WHERE t.name IS NOT NULL) as tags
FROM products p
LEFT JOIN categories c ON p.category_id = c.id
LEFT JOIN organizations org ON p.organization_id = org.id
LEFT JOIN product_tags pt ON p.id = pt.product_id
LEFT JOIN tags t ON pt.tag_id = t.id
GROUP BY p.id, c.name, c.slug, org.name;

-- User activity summary view
CREATE VIEW user_activity_summary AS
SELECT
    u.id,
    u.username,
    u.email,
    u.account_status,
    COUNT(DISTINCT om.organization_id) as organization_count,
    COUNT(DISTINCT o.id) as order_count,
    COALESCE(SUM(o.total_amount), 0) as total_spent,
    COUNT(DISTINCT r.id) as review_count,
    COUNT(DISTINCT uf.product_id) as favorite_count,
    u.last_login_at,
    u.created_at
FROM users u
LEFT JOIN organization_members om ON u.id = om.user_id
LEFT JOIN sales.orders o ON u.id = o.user_id AND o.status = 'completed'
LEFT JOIN reviews r ON u.id = r.user_id
LEFT JOIN user_favorites uf ON u.id = uf.user_id
GROUP BY u.id;

-- Category tree view
CREATE VIEW category_tree AS
WITH RECURSIVE category_path AS (
    SELECT
        id,
        slug,
        name,
        parent_id,
        1 as depth,
        ARRAY[name]::TEXT[] as path,
        ARRAY[id] as id_path
    FROM categories
    WHERE parent_id IS NULL

    UNION ALL

    SELECT
        c.id,
        c.slug,
        c.name,
        c.parent_id,
        cp.depth + 1,
        cp.path || c.name::TEXT,
        cp.id_path || c.id
    FROM categories c
    JOIN category_path cp ON c.parent_id = cp.id
)
SELECT
    id,
    slug,
    name,
    parent_id,
    depth,
    array_to_string(path, ' > ') as full_path,
    id_path
FROM category_path
ORDER BY id_path;

-- Daily sales report view
CREATE VIEW analytics.daily_sales_report AS
SELECT
    DATE(o.created_at) as sale_date,
    o.organization_id,
    org.name as organization_name,
    COUNT(DISTINCT o.id) as order_count,
    SUM(o.total_amount) as total_revenue,
    AVG(o.total_amount) as avg_order_value,
    SUM(o.discount_amount) as total_discounts,
    COUNT(DISTINCT o.user_id) as unique_customers
FROM sales.orders o
JOIN public.organizations org ON o.organization_id = org.id
WHERE o.status NOT IN ('cancelled', 'refunded')
GROUP BY DATE(o.created_at), o.organization_id, org.name;

-- ==========================================
-- RLS Policies
-- ==========================================
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE sales.orders ENABLE ROW LEVEL SECURITY;
ALTER TABLE sales.payments ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;

CREATE POLICY users_select_policy ON users
    FOR SELECT
    USING (true);

CREATE POLICY orders_select_policy ON sales.orders
    FOR SELECT
    USING (true);

CREATE POLICY payments_select_policy ON sales.payments
    FOR SELECT
    USING (true);

CREATE POLICY audit_logs_select_policy ON audit_logs
    FOR SELECT
    USING (true);
