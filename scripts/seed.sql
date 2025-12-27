-- Sample data for testing

-- Users
INSERT INTO users (username, email, full_name, phone, avatar_url, bio, location, company, job_title, website, github_url, twitter_handle, timezone, language_preference, password_hash, email_verified, phone_verified, two_factor_enabled, last_login_at, failed_login_attempts, account_status) VALUES
('alice', 'alice@example.com', 'Alice Johnson', '+1-555-0101', 'https://example.com/alice.jpg', 'Software engineer', 'San Francisco, CA', 'Tech Corp', 'Senior Engineer', 'https://alice.dev', 'https://github.com/alice', '@alice_dev', 'America/Los_Angeles', 'en', 'hashed_password_1', true, true, true, NOW() - INTERVAL '2 hours', 0, 'active'),
('bob', 'bob@example.com', 'Bob Smith', '+1-555-0102', 'https://example.com/bob.jpg', 'Product manager', 'New York, NY', 'Tech Corp', 'Product Manager', 'https://bob.dev', 'https://github.com/bob', '@bob_pm', 'America/New_York', 'en', 'hashed_password_2', true, false, false, NOW() - INTERVAL '1 day', 0, 'active'),
('charlie', 'charlie@example.com', 'Charlie Brown', '+1-555-0103', 'https://example.com/charlie.jpg', 'Design enthusiast', 'London, UK', 'Design Studio', 'UX Designer', 'https://charlie.design', 'https://github.com/charlie', '@charlie_design', 'Europe/London', 'en', 'hashed_password_3', true, true, false, NOW() - INTERVAL '3 days', 0, 'active'),
('diana', 'diana@example.com', 'Diana Prince', '+1-555-0104', 'https://example.com/diana.jpg', 'Data scientist', 'Austin, TX', 'Data Solutions', 'Data Scientist', 'https://diana.ai', 'https://github.com/diana', '@diana_data', 'America/Chicago', 'en', 'hashed_password_4', true, true, true, NOW() - INTERVAL '5 hours', 0, 'active'),
('eve', 'eve@example.com', 'Eve Wilson', '+1-555-0105', NULL, 'Developer advocate', 'Remote', 'Cloud Systems', 'Developer Advocate', 'https://eve.dev', 'https://github.com/eve', '@eve_advocate', 'UTC', 'ja', 'hashed_password_5', false, false, false, NULL, 0, 'active');

-- Organizations
INSERT INTO organizations (slug, name, description, logo_url, website, email, phone, country, city, business_type, industry, employee_count, founded_year, verified, subscription_tier, subscription_status, owner_id) VALUES
('tech-corp', 'Tech Corp', 'Leading technology company', 'https://example.com/tech-corp.png', 'https://techcorp.com', 'contact@techcorp.com', '+1-555-1000', 'United States', 'San Francisco', 'Corporation', 'Software Development', '500-1000', 2010, true, 'enterprise', 'active', 1),
('design-studio', 'Design Studio', 'Creative design agency', 'https://example.com/design-studio.png', 'https://designstudio.com', 'hello@designstudio.com', '+1-555-1001', 'United Kingdom', 'London', 'Partnership', 'Design & Creative', '50-100', 2015, true, 'professional', 'active', 3),
('data-solutions', 'Data Solutions', 'Advanced analytics platform', 'https://example.com/data-solutions.png', 'https://datasolutions.ai', 'info@datasolutions.ai', '+1-555-1002', 'United States', 'Austin', 'Corporation', 'Data Analytics', '100-250', 2018, true, 'professional', 'active', 4),
('cloud-systems', 'Cloud Systems', 'Cloud infrastructure provider', 'https://example.com/cloud-systems.png', 'https://cloudsystems.io', 'support@cloudsystems.io', '+1-555-1003', 'United States', 'Seattle', 'Corporation', 'Cloud Computing', '250-500', 2012, true, 'enterprise', 'active', 5);

-- Products
INSERT INTO products (sku, name, description, category, subcategory, price, cost_price, discount_price, tax_rate, stock_quantity, weight, color, size, material, image_url, supplier_id, organization_id, is_active, is_featured, rating, review_count) VALUES
('PROD-001', 'Cloud Storage Pro', 'Enterprise cloud storage solution', 'Software', 'Cloud Services', 999.99, 500.00, NULL, 10.00, 50, NULL, NULL, NULL, NULL, 'https://example.com/cloud-storage.jpg', 1, 1, true, true, 4.8, 156),
('PROD-002', 'Analytics Dashboard', 'Real-time analytics and reporting', 'Software', 'Analytics', 1499.99, 600.00, 1199.99, 10.00, 100, NULL, NULL, NULL, NULL, 'https://example.com/analytics.jpg', 2, 3, true, true, 4.9, 312),
('PROD-003', 'Design Pro Suite', 'Professional design tool suite', 'Software', 'Design Tools', 599.99, 250.00, NULL, 10.00, 75, NULL, NULL, NULL, NULL, 'https://example.com/design-suite.jpg', 3, 2, true, false, 4.7, 89),
('PROD-004', 'API Gateway', 'Scalable API management', 'Software', 'Infrastructure', 2499.99, 1000.00, NULL, 10.00, 30, NULL, NULL, NULL, NULL, 'https://example.com/api-gateway.jpg', 1, 1, true, true, 4.9, 245),
('PROD-005', 'Security Suite', 'Comprehensive security tools', 'Software', 'Security', 799.99, 350.00, 699.99, 10.00, 60, NULL, NULL, NULL, NULL, 'https://example.com/security.jpg', 5, 1, true, false, 4.6, 134),
('PROD-006', 'Mobile Dev Kit', 'Mobile application development', 'Software', 'Developer Tools', 399.99, 150.00, NULL, 10.00, 120, NULL, NULL, NULL, NULL, 'https://example.com/mobile-kit.jpg', 4, 3, true, true, 4.8, 267),
('PROD-007', 'Database Manager', 'Advanced database management', 'Software', 'Database', 1799.99, 700.00, NULL, 10.00, 40, NULL, NULL, NULL, NULL, 'https://example.com/db-manager.jpg', 2, 1, true, false, 4.7, 178),
('PROD-008', 'Content Hub', 'Digital content management', 'Software', 'Content Management', 549.99, 200.00, 499.99, 10.00, 90, NULL, NULL, NULL, NULL, 'https://example.com/content-hub.jpg', 3, 2, true, true, 4.8, 201);

-- Orders
INSERT INTO orders (order_number, user_id, organization_id, status, payment_status, shipping_status, total_amount, subtotal, tax_amount, discount_amount, billing_first_name, billing_last_name, billing_email, shipping_first_name, shipping_last_name, shipping_city, shipping_country, tracking_number, carrier, estimated_delivery_date, created_at) VALUES
('ORD-2025-0001', 1, 1, 'completed', 'paid', 'delivered', 1099.99, 999.99, 100.00, 0.00, 'Alice', 'Johnson', 'alice@example.com', 'Alice', 'Johnson', 'San Francisco', 'United States', 'TRK-12345', 'FedEx', NOW() + INTERVAL '7 days', NOW() - INTERVAL '10 days'),
('ORD-2025-0002', 2, 1, 'completed', 'paid', 'delivered', 2749.99, 2499.99, 250.00, 0.00, 'Bob', 'Smith', 'bob@example.com', 'Bob', 'Smith', 'New York', 'United States', 'TRK-12346', 'UPS', NOW() + INTERVAL '5 days', NOW() - INTERVAL '8 days'),
('ORD-2025-0003', 3, 2, 'completed', 'paid', 'delivered', 659.99, 599.99, 60.00, 0.00, 'Charlie', 'Brown', 'charlie@example.com', 'Charlie', 'Brown', 'London', 'United Kingdom', 'TRK-12347', 'DHL', NOW() + INTERVAL '10 days', NOW() - INTERVAL '15 days'),
('ORD-2025-0004', 4, 3, 'processing', 'paid', 'in_transit', 1319.99, 1199.99, 120.00, 0.00, 'Diana', 'Prince', 'diana@example.com', 'Diana', 'Prince', 'Austin', 'United States', 'TRK-12348', 'FedEx', NOW() + INTERVAL '3 days', NOW() - INTERVAL '2 days'),
('ORD-2025-0005', 1, 2, 'pending', 'unpaid', 'unshipped', 549.99, 499.99, 50.00, 0.00, 'Alice', 'Johnson', 'alice@example.com', 'Alice', 'Johnson', 'San Francisco', 'United States', NULL, NULL, NOW() + INTERVAL '14 days', NOW() - INTERVAL '1 day'),
('ORD-2025-0006', 5, 1, 'completed', 'paid', 'delivered', 879.99, 799.99, 80.00, 0.00, 'Eve', 'Wilson', 'eve@example.com', 'Eve', 'Wilson', 'Remote', 'United States', 'TRK-12349', 'UPS', NOW() + INTERVAL '8 days', NOW() - INTERVAL '12 days'),
('ORD-2025-0007', 2, 3, 'completed', 'paid', 'delivered', 1649.99, 1499.99, 150.00, 0.00, 'Bob', 'Smith', 'bob@example.com', 'Bob', 'Smith', 'New York', 'United States', 'TRK-12350', 'FedEx', NOW() + INTERVAL '6 days', NOW() - INTERVAL '9 days'),
('ORD-2025-0008', 3, 1, 'cancelled', 'refunded', 'returned', 1999.00, 1799.99, 199.01, 0.00, 'Charlie', 'Brown', 'charlie@example.com', 'Charlie', 'Brown', 'London', 'United Kingdom', NULL, NULL, NULL, NOW() - INTERVAL '20 days');

-- Order Items
INSERT INTO order_items (order_id, product_id, quantity, unit_price, line_total) VALUES
(1, 1, 1, 999.99, 999.99),
(2, 4, 1, 2499.99, 2499.99),
(3, 3, 1, 599.99, 599.99),
(4, 2, 1, 1199.99, 1199.99),
(5, 8, 1, 499.99, 499.99),
(6, 5, 1, 799.99, 799.99),
(7, 2, 1, 1499.99, 1499.99),
(8, 7, 1, 1799.99, 1799.99);

-- Payments
INSERT INTO payments (order_id, amount, payment_method, payment_status, transaction_id, gateway_response_code, authorization_code, cvv_verified, risk_score) VALUES
(1, 1099.99, 'credit_card', 'completed', 'TXN-001-2025', '00', 'AUTH-001', true, 5.2),
(2, 2749.99, 'credit_card', 'completed', 'TXN-002-2025', '00', 'AUTH-002', true, 3.8),
(3, 659.99, 'credit_card', 'completed', 'TXN-003-2025', '00', 'AUTH-003', true, 4.1),
(4, 1319.99, 'credit_card', 'completed', 'TXN-004-2025', '00', 'AUTH-004', true, 6.7),
(5, 549.99, 'bank_transfer', 'pending', NULL, NULL, NULL, NULL, 0.0),
(6, 879.99, 'credit_card', 'completed', 'TXN-006-2025', '00', 'AUTH-006', true, 4.5),
(7, 1649.99, 'credit_card', 'completed', 'TXN-007-2025', '00', 'AUTH-007', true, 3.2),
(8, 1999.00, 'credit_card', 'refunded', 'TXN-008-2025', '00', 'AUTH-008', true, 12.3);

-- Audit Logs
INSERT INTO audit_logs (user_id, table_name, operation, record_id, new_values) VALUES
(1, 'users', 'INSERT', 1, '{"username":"alice","email":"alice@example.com"}'),
(2, 'users', 'INSERT', 2, '{"username":"bob","email":"bob@example.com"}'),
(3, 'organizations', 'INSERT', 1, '{"name":"Tech Corp","slug":"tech-corp"}'),
(1, 'orders', 'INSERT', 1, '{"order_number":"ORD-2025-0001","status":"pending"}'),
(1, 'orders', 'UPDATE', 1, '{"status":"completed","payment_status":"paid"}'),
(4, 'products', 'INSERT', 1, '{"sku":"PROD-001","name":"Cloud Storage Pro"}'),
(2, 'orders', 'INSERT', 2, '{"order_number":"ORD-2025-0002","status":"pending"}'),
(2, 'payments', 'INSERT', 1, '{"transaction_id":"TXN-001-2025","payment_status":"completed"}');

-- Settings
INSERT INTO settings (organization_id, setting_key, setting_value, data_type, is_public) VALUES
(1, 'theme_color', '#0066CC', 'string', true),
(1, 'max_api_calls', '10000', 'integer', false),
(1, 'email_notifications_enabled', 'true', 'boolean', false),
(1, 'timezone', 'America/Los_Angeles', 'string', false),
(2, 'theme_color', '#FF6B6B', 'string', true),
(2, 'max_api_calls', '5000', 'integer', false),
(3, 'theme_color', '#4ECDC4', 'string', true),
(3, 'max_api_calls', '15000', 'integer', false),
(4, 'theme_color', '#95E1D3', 'string', true),
(4, 'max_api_calls', '20000', 'integer', false);
