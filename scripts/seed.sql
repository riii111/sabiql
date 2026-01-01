-- ==========================================
-- Sample data for testing (enhanced version)
-- ==========================================

-- ==========================================
-- Users (with manager hierarchy)
-- ==========================================
INSERT INTO users (username, email, full_name, phone, avatar_url, bio, location, company, job_title, website, github_url, twitter_handle, timezone, language_preference, password_hash, email_verified, phone_verified, two_factor_enabled, last_login_at, failed_login_attempts, account_status, manager_id) VALUES
('alice', 'alice@example.com', 'Alice Johnson', '+1-555-0101', 'https://example.com/alice.jpg', 'Software engineer', 'San Francisco, CA', 'Tech Corp', 'Senior Engineer', 'https://alice.dev', 'https://github.com/alice', '@alice_dev', 'America/Los_Angeles', 'en', 'hashed_password_1', true, true, true, NOW() - INTERVAL '2 hours', 0, 'active', NULL),
('bob', 'bob@example.com', 'Bob Smith', '+1-555-0102', 'https://example.com/bob.jpg', 'Product manager', 'New York, NY', 'Tech Corp', 'Product Manager', 'https://bob.dev', 'https://github.com/bob', '@bob_pm', 'America/New_York', 'en', 'hashed_password_2', true, false, false, NOW() - INTERVAL '1 day', 0, 'active', 1),
('charlie', 'charlie@example.com', 'Charlie Brown', '+1-555-0103', 'https://example.com/charlie.jpg', 'Design enthusiast', 'London, UK', 'Design Studio', 'UX Designer', 'https://charlie.design', 'https://github.com/charlie', '@charlie_design', 'Europe/London', 'en', 'hashed_password_3', true, true, false, NOW() - INTERVAL '3 days', 0, 'active', NULL),
('diana', 'diana@example.com', 'Diana Prince', '+1-555-0104', 'https://example.com/diana.jpg', 'Data scientist', 'Austin, TX', 'Data Solutions', 'Data Scientist', 'https://diana.ai', 'https://github.com/diana', '@diana_data', 'America/Chicago', 'en', 'hashed_password_4', true, true, true, NOW() - INTERVAL '5 hours', 0, 'active', NULL),
('eve', 'eve@example.com', 'Eve Wilson', '+1-555-0105', NULL, 'Developer advocate', 'Remote', 'Cloud Systems', 'Developer Advocate', 'https://eve.dev', 'https://github.com/eve', '@eve_advocate', 'UTC', 'ja', 'hashed_password_5', false, false, false, NULL, 0, 'active', NULL),
('frank', 'frank.li@example.com', 'Frank Li', '+1-555-0106', 'https://example.com/frank.jpg', 'Backend engineer', 'Seattle, WA', 'Cloud Systems', 'Backend Engineer', 'https://frank.li', 'https://github.com/frankli', '@frank_backend', 'America/Los_Angeles', 'en', 'hashed_password_6', true, true, false, NOW() - INTERVAL '12 hours', 1, 'active', 5),
('grace', 'grace.park@example.com', 'Grace Park', '+1-555-0107', 'https://example.com/grace.jpg', 'QA lead', 'Toronto, CA', 'Tech Corp', 'QA Lead', 'https://grace.qa', 'https://github.com/gracepark', '@grace_tests', 'America/Toronto', 'en', 'hashed_password_7', true, true, false, NOW() - INTERVAL '4 days', 0, 'active', 1),
('henry', 'henry.ng@example.com', 'Henry Ng', '+1-555-0108', NULL, 'DevOps specialist', 'Vancouver, CA', 'Cloud Systems', 'DevOps Specialist', 'https://henry.ops', 'https://github.com/henryng', '@henry_ops', 'America/Vancouver', 'en', 'hashed_password_8', true, false, false, NOW() - INTERVAL '7 days', 2, 'active', 5),
('isabel', 'isabel.chen@example.com', 'Isabel Chen', '+1-555-0109', 'https://example.com/isabel.jpg', 'Marketing strategist', 'Sydney, AU', 'Data Solutions', 'Marketing Lead', 'https://isabel.marketing', 'https://github.com/isabelchen', '@isabelgrowth', 'Australia/Sydney', 'en', 'hashed_password_9', true, false, false, NOW() - INTERVAL '9 hours', 0, 'active', 4),
('jack', 'jack.turner@example.com', 'Jack Turner', '+1-555-0110', NULL, 'Support engineer', 'Denver, CO', 'Tech Corp', 'Support Engineer', 'https://jack.support', 'https://github.com/jackturner', '@jack_support', 'America/Denver', 'en', 'hashed_password_10', true, true, false, NOW() - INTERVAL '2 days', 0, 'active', 1),
('kim', 'kim.min@example.com', 'Kim Min', '+1-555-0111', 'https://example.com/kim.jpg', 'Data analyst', 'Seoul, KR', 'Data Solutions', 'Data Analyst', 'https://kim.data', 'https://github.com/kimmin', '@kim_data', 'Asia/Seoul', 'ko', 'hashed_password_11', true, true, false, NOW() - INTERVAL '6 days', 0, 'active', 4),
('leo', 'leo.martinez@example.com', 'Leo Martinez', '+1-555-0112', NULL, 'Sales director', 'Miami, FL', 'Cloud Systems', 'Sales Director', 'https://leo.sales', 'https://github.com/leomartinez', '@leo_sales', 'America/New_York', 'en', 'hashed_password_12', true, true, false, NOW() - INTERVAL '3 hours', 0, 'active', NULL),
('maya', 'maya.patel@example.com', 'Maya Patel', '+1-555-0113', 'https://example.com/maya.jpg', 'Product designer', 'Berlin, DE', 'Design Studio', 'Product Designer', 'https://maya.design', 'https://github.com/mayapatel', '@maya_design', 'Europe/Berlin', 'en', 'hashed_password_13', true, true, true, NOW() - INTERVAL '11 days', 0, 'active', 3),
('nora', 'nora.hughes@example.com', 'Nora Hughes', '+1-555-0114', NULL, 'HR manager', 'Chicago, IL', 'Tech Corp', 'HR Manager', 'https://nora.hr', 'https://github.com/norahughes', '@nora_hr', 'America/Chicago', 'en', 'hashed_password_14', true, false, false, NOW() - INTERVAL '14 days', 1, 'active', NULL),
('owen', 'owen.kim@example.com', 'Owen Kim', '+1-555-0115', 'https://example.com/owen.jpg', 'Security engineer', 'Boston, MA', 'Cloud Systems', 'Security Engineer', 'https://owen.security', 'https://github.com/owenkim', '@owen_sec', 'America/New_York', 'en', 'hashed_password_15', true, true, true, NOW() - INTERVAL '1 day', 0, 'active', 5),
('peter', 'peter.wang@example.com', 'Peter Wang', '+1-555-0116', 'https://example.com/peter.jpg', 'Frontend developer', 'Portland, OR', 'Tech Corp', 'Frontend Developer', 'https://peter.ui', 'https://github.com/peterwang', '@peter_fe', 'America/Los_Angeles', 'en', 'hashed_password_16', true, true, false, NOW() - INTERVAL '8 hours', 0, 'active', 1),
('quinn', 'quinn.taylor@example.com', 'Quinn Taylor', '+1-555-0117', NULL, 'Mobile developer', 'Austin, TX', 'Tech Corp', 'Mobile Developer', 'https://quinn.mobile', 'https://github.com/quinntaylor', '@quinn_mobile', 'America/Chicago', 'en', 'hashed_password_17', true, false, false, NOW() - INTERVAL '5 days', 0, 'active', 1),
('rachel', 'rachel.green@example.com', 'Rachel Green', '+1-555-0118', 'https://example.com/rachel.jpg', 'Content strategist', 'New York, NY', 'Design Studio', 'Content Lead', 'https://rachel.content', 'https://github.com/rachelgreen', '@rachel_content', 'America/New_York', 'en', 'hashed_password_18', true, true, false, NOW() - INTERVAL '2 days', 0, 'active', 3),
('sam', 'sam.wilson@example.com', 'Sam Wilson', '+1-555-0119', NULL, 'Infrastructure engineer', 'Seattle, WA', 'Cloud Systems', 'Infra Engineer', 'https://sam.infra', 'https://github.com/samwilson', '@sam_infra', 'America/Los_Angeles', 'en', 'hashed_password_19', true, true, true, NOW() - INTERVAL '6 hours', 0, 'active', 8),
('tina', 'tina.rodriguez@example.com', 'Tina Rodriguez', '+1-555-0120', 'https://example.com/tina.jpg', 'Business analyst', 'Miami, FL', 'Data Solutions', 'Business Analyst', 'https://tina.ba', 'https://github.com/tinarodriguez', '@tina_ba', 'America/New_York', 'en', 'hashed_password_20', true, true, false, NOW() - INTERVAL '10 days', 0, 'active', 4);

-- ==========================================
-- Sessions
-- ==========================================
INSERT INTO sessions (user_id, token_hash, ip_address, user_agent, device_type, last_activity_at, expires_at) VALUES
(1, 'hash_session_001', '192.168.1.100', 'Mozilla/5.0 Chrome/120', 'desktop', NOW() - INTERVAL '30 minutes', NOW() + INTERVAL '7 days'),
(1, 'hash_session_002', '10.0.0.50', 'Safari/17.0 Mobile', 'mobile', NOW() - INTERVAL '2 hours', NOW() + INTERVAL '7 days'),
(2, 'hash_session_003', '192.168.1.101', 'Mozilla/5.0 Firefox/121', 'desktop', NOW() - INTERVAL '1 day', NOW() + INTERVAL '6 days'),
(3, 'hash_session_004', '172.16.0.25', 'Mozilla/5.0 Chrome/120', 'desktop', NOW() - INTERVAL '3 hours', NOW() + INTERVAL '7 days'),
(4, 'hash_session_005', '192.168.2.50', 'Edge/120', 'desktop', NOW() - INTERVAL '5 hours', NOW() + INTERVAL '7 days'),
(5, 'hash_session_006', '10.10.10.100', 'Safari/17.0', 'desktop', NOW() - INTERVAL '12 hours', NOW() + INTERVAL '5 days'),
(6, 'hash_session_007', '192.168.1.150', 'Mozilla/5.0 Chrome/120', 'desktop', NOW() - INTERVAL '2 days', NOW() + INTERVAL '5 days'),
(7, 'hash_session_008', '172.20.0.75', 'Safari/17.0 Mobile', 'tablet', NOW() - INTERVAL '4 hours', NOW() + INTERVAL '7 days');

-- ==========================================
-- API Keys
-- ==========================================
INSERT INTO api_keys (user_id, name, key_hash, key_prefix, scopes, rate_limit, last_used_at, expires_at, is_active) VALUES
(1, 'Production API', 'hash_key_001', 'sk_prod_', ARRAY['read', 'write', 'admin'], 10000, NOW() - INTERVAL '1 hour', NOW() + INTERVAL '1 year', true),
(1, 'Development API', 'hash_key_002', 'sk_dev_', ARRAY['read', 'write'], 5000, NOW() - INTERVAL '30 minutes', NOW() + INTERVAL '6 months', true),
(2, 'CI/CD Integration', 'hash_key_003', 'sk_ci_', ARRAY['read', 'deploy'], 2000, NOW() - INTERVAL '2 hours', NOW() + INTERVAL '3 months', true),
(4, 'Analytics Key', 'hash_key_004', 'sk_ana_', ARRAY['read'], 1000, NOW() - INTERVAL '1 day', NULL, true),
(5, 'Old Key', 'hash_key_005', 'sk_old_', ARRAY['read'], 500, NOW() - INTERVAL '90 days', NOW() - INTERVAL '30 days', false),
(6, 'Backend Service', 'hash_key_006', 'sk_svc_', ARRAY['read', 'write'], 15000, NOW() - INTERVAL '3 hours', NOW() + INTERVAL '2 years', true),
(8, 'DevOps Automation', 'hash_key_007', 'sk_ops_', ARRAY['read', 'write', 'deploy'], 8000, NOW() - INTERVAL '6 hours', NOW() + INTERVAL '1 year', true);

-- ==========================================
-- Password Reset Tokens
-- ==========================================
INSERT INTO password_reset_tokens (user_id, token_hash, used_at, expires_at) VALUES
(5, 'reset_hash_001', NULL, NOW() + INTERVAL '1 hour'),
(8, 'reset_hash_002', NOW() - INTERVAL '2 days', NOW() - INTERVAL '1 day'),
(14, 'reset_hash_003', NULL, NOW() + INTERVAL '30 minutes');

-- ==========================================
-- Organizations (with parent hierarchy)
-- ==========================================
INSERT INTO organizations (slug, name, description, logo_url, website, email, phone, country, city, business_type, industry, employee_count, founded_year, verified, subscription_tier, subscription_status, owner_id, parent_organization_id) VALUES
('tech-corp', 'Tech Corp', 'Leading technology company', 'https://example.com/tech-corp.png', 'https://techcorp.com', 'contact@techcorp.com', '+1-555-1000', 'United States', 'San Francisco', 'Corporation', 'Software Development', '500-1000', 2010, true, 'enterprise', 'active', 1, NULL),
('design-studio', 'Design Studio', 'Creative design agency', 'https://example.com/design-studio.png', 'https://designstudio.com', 'hello@designstudio.com', '+1-555-1001', 'United Kingdom', 'London', 'Partnership', 'Design & Creative', '50-100', 2015, true, 'professional', 'active', 3, NULL),
('data-solutions', 'Data Solutions', 'Advanced analytics platform', 'https://example.com/data-solutions.png', 'https://datasolutions.ai', 'info@datasolutions.ai', '+1-555-1002', 'United States', 'Austin', 'Corporation', 'Data Analytics', '100-250', 2018, true, 'professional', 'active', 4, NULL),
('cloud-systems', 'Cloud Systems', 'Cloud infrastructure provider', 'https://example.com/cloud-systems.png', 'https://cloudsystems.io', 'support@cloudsystems.io', '+1-555-1003', 'United States', 'Seattle', 'Corporation', 'Cloud Computing', '250-500', 2012, true, 'enterprise', 'active', 5, NULL),
('greenfield-ops', 'Greenfield Ops', 'Managed DevOps and SRE services', 'https://example.com/greenfield-ops.png', 'https://greenfieldops.com', 'hello@greenfieldops.com', '+1-555-1004', 'United States', 'Portland', 'LLC', 'DevOps Services', '25-50', 2019, true, 'professional', 'active', 6, NULL),
('northwind-retail', 'Northwind Retail', 'Omnichannel retail platform', 'https://example.com/northwind.png', 'https://northwindretail.com', 'support@northwindretail.com', '+1-555-1005', 'United States', 'Chicago', 'Corporation', 'Retail Tech', '250-500', 2011, true, 'enterprise', 'active', 7, NULL),
('brightline-labs', 'Brightline Labs', 'Applied AI research studio', 'https://example.com/brightline.png', 'https://brightlinelabs.ai', 'contact@brightlinelabs.ai', '+1-555-1006', 'Canada', 'Toronto', 'Corporation', 'Artificial Intelligence', '50-100', 2016, true, 'professional', 'active', 8, NULL),
('riverbend-media', 'Riverbend Media', 'Streaming media production', 'https://example.com/riverbend.png', 'https://riverbendmedia.com', 'studio@riverbendmedia.com', '+1-555-1007', 'United Kingdom', 'Manchester', 'Corporation', 'Media & Entertainment', '100-250', 2013, true, 'professional', 'active', 9, NULL),
('atlas-finance', 'Atlas Finance', 'Risk analytics for fintech', 'https://example.com/atlas-finance.png', 'https://atlasfinance.io', 'hello@atlasfinance.io', '+1-555-1008', 'United States', 'New York', 'Corporation', 'Fintech', '100-250', 2017, true, 'enterprise', 'active', 10, NULL),
('openlane-logistics', 'Openlane Logistics', 'Last-mile logistics software', 'https://example.com/openlane.png', 'https://openlane.ai', 'ops@openlane.ai', '+1-555-1009', 'United States', 'Atlanta', 'Corporation', 'Logistics', '50-100', 2020, true, 'starter', 'active', 11, NULL),
('zenith-health', 'Zenith Health', 'Healthcare data platform', 'https://example.com/zenith-health.png', 'https://zenithhealth.io', 'info@zenithhealth.io', '+1-555-1010', 'United States', 'San Diego', 'Corporation', 'Health Tech', '250-500', 2014, true, 'enterprise', 'active', 12, NULL),
('summit-edtech', 'Summit EdTech', 'Learning analytics and content delivery', 'https://example.com/summit-edtech.png', 'https://summitedtech.com', 'hello@summitedtech.com', '+1-555-1011', 'United States', 'Boston', 'Corporation', 'Education Technology', '50-100', 2019, true, 'professional', 'active', 15, NULL),
('tech-corp-asia', 'Tech Corp Asia', 'Asia-Pacific division of Tech Corp', 'https://example.com/tech-corp-asia.png', 'https://asia.techcorp.com', 'asia@techcorp.com', '+81-3-1234-5678', 'Japan', 'Tokyo', 'Corporation', 'Software Development', '100-250', 2018, true, 'enterprise', 'active', 1, 1),
('tech-corp-europe', 'Tech Corp Europe', 'European division of Tech Corp', 'https://example.com/tech-corp-eu.png', 'https://eu.techcorp.com', 'europe@techcorp.com', '+44-20-7123-4567', 'United Kingdom', 'London', 'Corporation', 'Software Development', '100-250', 2017, true, 'enterprise', 'active', 1, 1);

-- ==========================================
-- Organization Members (many-to-many)
-- ==========================================
INSERT INTO organization_members (organization_id, user_id, role, permissions, invited_by, joined_at) VALUES
(1, 1, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '3 years'),
(1, 2, 'admin', ARRAY['admin', 'member_management'], 1, NOW() - INTERVAL '2 years'),
(1, 7, 'member', ARRAY['read', 'write'], 1, NOW() - INTERVAL '1 year'),
(1, 10, 'member', ARRAY['read', 'write', 'support'], 2, NOW() - INTERVAL '6 months'),
(1, 16, 'member', ARRAY['read', 'write'], 2, NOW() - INTERVAL '8 months'),
(1, 17, 'member', ARRAY['read', 'write'], 2, NOW() - INTERVAL '4 months'),
(2, 3, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '2 years'),
(2, 13, 'admin', ARRAY['admin', 'member_management'], 3, NOW() - INTERVAL '1 year'),
(2, 18, 'member', ARRAY['read', 'write'], 3, NOW() - INTERVAL '6 months'),
(3, 4, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '2 years'),
(3, 9, 'admin', ARRAY['admin', 'analytics'], 4, NOW() - INTERVAL '1 year'),
(3, 11, 'member', ARRAY['read', 'write', 'analytics'], 4, NOW() - INTERVAL '8 months'),
(3, 20, 'member', ARRAY['read', 'analytics'], 9, NOW() - INTERVAL '3 months'),
(4, 5, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '3 years'),
(4, 6, 'admin', ARRAY['admin', 'member_management'], 5, NOW() - INTERVAL '2 years'),
(4, 8, 'member', ARRAY['read', 'write', 'deploy'], 5, NOW() - INTERVAL '1 year'),
(4, 15, 'member', ARRAY['read', 'write', 'security'], 6, NOW() - INTERVAL '6 months'),
(4, 19, 'member', ARRAY['read', 'write', 'deploy'], 8, NOW() - INTERVAL '4 months'),
(5, 6, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '1 year'),
(6, 7, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '4 years'),
(7, 8, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '2 years'),
(8, 9, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '3 years'),
(9, 10, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '2 years'),
(10, 11, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '1 year'),
(11, 12, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '3 years'),
(12, 15, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '1 year'),
(13, 1, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '2 years'),
(14, 1, 'owner', ARRAY['admin', 'billing', 'member_management'], NULL, NOW() - INTERVAL '2 years');

-- ==========================================
-- Departments (with hierarchy)
-- ==========================================
INSERT INTO departments (organization_id, name, code, description, parent_department_id, manager_id, budget, headcount, is_active) VALUES
(1, 'Engineering', 'ENG', 'Software engineering department', NULL, 1, 5000000.00, 150, true),
(1, 'Backend Team', 'ENG-BE', 'Backend development', 1, 6, 1500000.00, 40, true),
(1, 'Frontend Team', 'ENG-FE', 'Frontend development', 1, 16, 1200000.00, 35, true),
(1, 'Mobile Team', 'ENG-MOB', 'Mobile app development', 1, 17, 1000000.00, 25, true),
(1, 'QA Team', 'ENG-QA', 'Quality assurance', 1, 7, 800000.00, 20, true),
(1, 'Product', 'PROD', 'Product management', NULL, 2, 1500000.00, 25, true),
(1, 'Support', 'SUP', 'Customer support', NULL, 10, 800000.00, 30, true),
(1, 'HR', 'HR', 'Human resources', NULL, 14, 500000.00, 10, true),
(2, 'Design', 'DES', 'Design department', NULL, 3, 800000.00, 20, true),
(2, 'UX Research', 'DES-UX', 'User experience research', 9, 13, 300000.00, 8, true),
(2, 'Visual Design', 'DES-VIS', 'Visual and brand design', 9, 18, 300000.00, 8, true),
(3, 'Analytics', 'ANA', 'Data analytics', NULL, 4, 1200000.00, 30, true),
(3, 'Data Engineering', 'ANA-DE', 'Data engineering team', 12, 11, 600000.00, 15, true),
(4, 'Infrastructure', 'INF', 'Cloud infrastructure', NULL, 5, 2000000.00, 40, true),
(4, 'DevOps', 'INF-OPS', 'DevOps team', 14, 8, 800000.00, 15, true),
(4, 'Security', 'INF-SEC', 'Security team', 14, 15, 600000.00, 10, true);

-- ==========================================
-- Categories (hierarchical)
-- ==========================================
INSERT INTO categories (slug, name, description, icon, parent_id, sort_order, is_active) VALUES
('software', 'Software', 'Software products and services', 'code', NULL, 1, true),
('cloud-services', 'Cloud Services', 'Cloud-based solutions', 'cloud', 1, 1, true),
('analytics', 'Analytics', 'Analytics and reporting tools', 'chart-bar', 1, 2, true),
('developer-tools', 'Developer Tools', 'Tools for software developers', 'wrench', 1, 3, true),
('security', 'Security', 'Security and compliance tools', 'shield', 1, 4, true),
('design-tools', 'Design Tools', 'Design and creative software', 'palette', 1, 5, true),
('infrastructure', 'Infrastructure', 'Infrastructure management', 'server', 2, 1, true),
('storage', 'Storage', 'Cloud storage solutions', 'database', 2, 2, true),
('compute', 'Compute', 'Cloud compute resources', 'cpu', 2, 3, true),
('business-intelligence', 'Business Intelligence', 'BI and dashboards', 'presentation-chart-bar', 3, 1, true),
('data-science', 'Data Science', 'ML and data science tools', 'beaker', 3, 2, true),
('marketing-analytics', 'Marketing Analytics', 'Marketing performance', 'megaphone', 3, 3, true),
('ides', 'IDEs', 'Integrated development environments', 'code-bracket', 4, 1, true),
('apis', 'APIs', 'API management and gateways', 'arrows-right-left', 4, 2, true),
('testing', 'Testing', 'Testing and QA tools', 'check-circle', 4, 3, true),
('identity', 'Identity', 'Identity and access management', 'user-circle', 5, 1, true),
('threat-detection', 'Threat Detection', 'Security monitoring', 'eye', 5, 2, true),
('compliance', 'Compliance', 'Compliance and audit', 'clipboard-check', 5, 3, true),
('prototyping', 'Prototyping', 'Design prototyping tools', 'cursor-arrow-rays', 6, 1, true),
('collaboration', 'Collaboration', 'Design collaboration', 'users', 6, 2, true),
('hardware', 'Hardware', 'Physical products', 'computer-desktop', NULL, 2, true),
('networking', 'Networking', 'Network equipment', 'globe', 21, 1, true),
('peripherals', 'Peripherals', 'Computer peripherals', 'square-3-stack-3d', 21, 2, true);

-- ==========================================
-- Tags
-- ==========================================
INSERT INTO tags (slug, name, color) VALUES
('enterprise', 'Enterprise', '#2563EB'),
('startup-friendly', 'Startup Friendly', '#10B981'),
('open-source', 'Open Source', '#8B5CF6'),
('saas', 'SaaS', '#F59E0B'),
('on-premise', 'On-Premise', '#6B7280'),
('api-first', 'API First', '#EC4899'),
('no-code', 'No-Code', '#14B8A6'),
('ai-powered', 'AI Powered', '#EF4444'),
('real-time', 'Real-time', '#3B82F6'),
('scalable', 'Scalable', '#22C55E'),
('secure', 'Secure', '#F97316'),
('mobile-ready', 'Mobile Ready', '#A855F7'),
('integrations', 'Integrations', '#64748B'),
('free-tier', 'Free Tier', '#06B6D4'),
('premium', 'Premium', '#D97706');

-- ==========================================
-- Products
-- ==========================================
INSERT INTO products (sku, name, description, category_id, price, cost_price, discount_price, tax_rate, stock_quantity, weight, color, size, material, image_url, supplier_id, organization_id, is_active, is_featured, rating, review_count) VALUES
('PROD-001', 'Cloud Storage Pro', 'Enterprise cloud storage solution', 8, 999.99, 500.00, NULL, 10.00, 50, NULL, NULL, NULL, NULL, 'https://example.com/cloud-storage.jpg', 1, 1, true, true, 4.8, 156),
('PROD-002', 'Analytics Dashboard', 'Real-time analytics and reporting', 10, 1499.99, 600.00, 1199.99, 10.00, 100, NULL, NULL, NULL, NULL, 'https://example.com/analytics.jpg', 2, 3, true, true, 4.9, 312),
('PROD-003', 'Design Pro Suite', 'Professional design tool suite', 19, 599.99, 250.00, NULL, 10.00, 75, NULL, NULL, NULL, NULL, 'https://example.com/design-suite.jpg', 3, 2, true, false, 4.7, 89),
('PROD-004', 'API Gateway', 'Scalable API management', 14, 2499.99, 1000.00, NULL, 10.00, 30, NULL, NULL, NULL, NULL, 'https://example.com/api-gateway.jpg', 1, 1, true, true, 4.9, 245),
('PROD-005', 'Security Suite', 'Comprehensive security tools', 17, 799.99, 350.00, 699.99, 10.00, 60, NULL, NULL, NULL, NULL, 'https://example.com/security.jpg', 5, 1, true, false, 4.6, 134),
('PROD-006', 'Mobile Dev Kit', 'Mobile application development', 4, 399.99, 150.00, NULL, 10.00, 120, NULL, NULL, NULL, NULL, 'https://example.com/mobile-kit.jpg', 4, 3, true, true, 4.8, 267),
('PROD-007', 'Database Manager', 'Advanced database management', 8, 1799.99, 700.00, NULL, 10.00, 40, NULL, NULL, NULL, NULL, 'https://example.com/db-manager.jpg', 2, 1, true, false, 4.7, 178),
('PROD-008', 'Content Hub', 'Digital content management', 20, 549.99, 200.00, 499.99, 10.00, 90, NULL, NULL, NULL, NULL, 'https://example.com/content-hub.jpg', 3, 2, true, true, 4.8, 201),
('PROD-009', 'Compliance Monitor', 'Automated compliance and audit tracking', 18, 899.99, 380.00, NULL, 10.00, 70, NULL, NULL, NULL, NULL, 'https://example.com/compliance.jpg', 10, 10, true, false, 4.6, 94),
('PROD-010', 'Retail Insights', 'Customer behavior analytics for retail', 12, 1299.99, 520.00, 999.99, 10.00, 85, NULL, NULL, NULL, NULL, 'https://example.com/retail-insights.jpg', 7, 7, true, true, 4.7, 121),
('PROD-011', 'Media Pipeline', 'Video processing and workflow automation', 2, 2199.99, 980.00, NULL, 10.00, 35, NULL, NULL, NULL, NULL, 'https://example.com/media-pipeline.jpg', 9, 9, true, false, 4.5, 63),
('PROD-012', 'AI Notebook', 'Collaborative notebooks for ML teams', 11, 1599.99, 700.00, 1399.99, 10.00, 60, NULL, NULL, NULL, NULL, 'https://example.com/ai-notebook.jpg', 8, 8, true, true, 4.8, 204),
('PROD-013', 'Logistics Route Optimizer', 'Route planning and optimization', 3, 1099.99, 450.00, NULL, 10.00, 55, NULL, NULL, NULL, NULL, 'https://example.com/route-optimizer.jpg', 11, 11, true, false, 4.6, 88),
('PROD-014', 'Health Data Vault', 'HIPAA-ready data storage', 18, 1899.99, 820.00, NULL, 10.00, 40, NULL, NULL, NULL, NULL, 'https://example.com/health-vault.jpg', 12, 12, true, true, 4.9, 175),
('PROD-015', 'Edge Monitor', 'Edge device observability suite', 7, 1399.99, 600.00, 1199.99, 10.00, 50, NULL, NULL, NULL, NULL, 'https://example.com/edge-monitor.jpg', 6, 6, true, false, 4.4, 57),
('PROD-016', 'Team Workspace', 'Secure collaboration workspace', 20, 699.99, 260.00, NULL, 10.00, 95, NULL, NULL, NULL, NULL, 'https://example.com/team-workspace.jpg', 7, 7, true, true, 4.7, 143),
('PROD-017', 'API Shield', 'API threat detection and protection', 17, 999.99, 420.00, NULL, 10.00, 65, NULL, NULL, NULL, NULL, 'https://example.com/api-shield.jpg', 15, 1, true, true, 4.8, 162),
('PROD-018', 'Design Sprint Kit', 'Rapid prototyping toolkit', 19, 449.99, 170.00, NULL, 10.00, 110, NULL, NULL, NULL, NULL, 'https://example.com/design-sprint.jpg', 13, 2, true, false, 4.6, 72),
('PROD-019', 'Growth Signals', 'Marketing attribution platform', 12, 1199.99, 480.00, 999.99, 10.00, 80, NULL, NULL, NULL, NULL, 'https://example.com/growth-signals.jpg', 9, 10, true, true, 4.7, 109),
('PROD-020', 'Data Clean Room', 'Privacy-safe data sharing', 11, 1999.99, 850.00, NULL, 10.00, 45, NULL, NULL, NULL, NULL, 'https://example.com/clean-room.jpg', 8, 3, true, false, 4.8, 132),
('PROD-021', 'Ops Runbook', 'On-call automation and runbooks', 4, 749.99, 300.00, NULL, 10.00, 90, NULL, NULL, NULL, NULL, 'https://example.com/ops-runbook.jpg', 6, 6, true, false, 4.5, 84),
('PROD-022', 'Customer Journey Map', 'End-to-end journey analytics', 12, 1399.99, 560.00, 1199.99, 10.00, 70, NULL, NULL, NULL, NULL, 'https://example.com/journey-map.jpg', 10, 10, true, true, 4.6, 98),
('PROD-023', 'Realtime Alerts', 'Low-latency alerting platform', 7, 899.99, 360.00, NULL, 10.00, 120, NULL, NULL, NULL, NULL, 'https://example.com/realtime-alerts.jpg', 11, 11, true, true, 4.7, 156),
('PROD-024', 'Insight Studio', 'Self-serve BI for teams', 10, 1499.99, 610.00, NULL, 10.00, 75, NULL, NULL, NULL, NULL, 'https://example.com/insight-studio.jpg', 12, 12, true, false, 4.6, 119),
('PROD-025', 'Inventory Pulse', 'Real-time inventory visibility', 3, 1099.99, 460.00, NULL, 10.00, 65, NULL, NULL, NULL, NULL, 'https://example.com/inventory-pulse.jpg', 7, 7, true, true, 4.5, 78),
('PROD-026', 'Support Console', 'Unified customer support workspace', 20, 899.99, 360.00, 799.99, 10.00, 90, NULL, NULL, NULL, NULL, 'https://example.com/support-console.jpg', 6, 6, true, false, 4.4, 61),
('PROD-027', 'Fraud Sentinel', 'Transaction fraud detection', 17, 1699.99, 740.00, NULL, 10.00, 55, NULL, NULL, NULL, NULL, 'https://example.com/fraud-sentinel.jpg', 10, 10, true, true, 4.7, 104),
('PROD-028', 'Clinic Scheduler', 'Appointment and capacity planning', 3, 1299.99, 520.00, NULL, 10.00, 70, NULL, NULL, NULL, NULL, 'https://example.com/clinic-scheduler.jpg', 12, 12, true, false, 4.6, 83),
('PROD-029', 'Creative Proof', 'Design review and approval workflows', 20, 599.99, 240.00, NULL, 10.00, 105, NULL, NULL, NULL, NULL, 'https://example.com/creative-proof.jpg', 2, 2, true, false, 4.5, 58),
('PROD-030', 'Logistics Control Tower', 'End-to-end shipment visibility', 3, 1899.99, 820.00, NULL, 10.00, 45, NULL, NULL, NULL, NULL, 'https://example.com/control-tower.jpg', 11, 11, true, true, 4.8, 131);

-- ==========================================
-- Product Tags (many-to-many)
-- ==========================================
INSERT INTO product_tags (product_id, tag_id) VALUES
(1, 1), (1, 4), (1, 10), (1, 11),
(2, 1), (2, 4), (2, 8), (2, 9),
(3, 2), (3, 4), (3, 12), (3, 14),
(4, 1), (4, 6), (4, 9), (4, 10),
(5, 1), (5, 11), (5, 4),
(6, 2), (6, 4), (6, 12), (6, 14),
(7, 1), (7, 4), (7, 10),
(8, 2), (8, 4), (8, 7), (8, 13),
(9, 1), (9, 11), (9, 5),
(10, 1), (10, 8), (10, 9),
(11, 1), (11, 4), (11, 10),
(12, 2), (12, 3), (12, 8), (12, 14),
(13, 2), (13, 8), (13, 9),
(14, 1), (14, 11), (14, 15),
(15, 1), (15, 9), (15, 10),
(16, 2), (16, 4), (16, 13), (16, 14),
(17, 1), (17, 6), (17, 11),
(18, 2), (18, 7), (18, 14),
(19, 1), (19, 8), (19, 9),
(20, 1), (20, 11), (20, 15),
(21, 2), (21, 4), (21, 13),
(22, 1), (22, 8), (22, 9),
(23, 2), (23, 4), (23, 9), (23, 14),
(24, 1), (24, 7), (24, 8),
(25, 2), (25, 9), (25, 13),
(26, 2), (26, 4), (26, 13),
(27, 1), (27, 8), (27, 11),
(28, 1), (28, 4), (28, 11),
(29, 2), (29, 7), (29, 13),
(30, 1), (30, 9), (30, 10);

-- ==========================================
-- User Favorites (many-to-many)
-- ==========================================
INSERT INTO user_favorites (user_id, product_id, notes) VALUES
(1, 2, 'Great for team analytics'),
(1, 4, 'Need to evaluate for API project'),
(1, 17, 'Security review pending'),
(2, 1, NULL),
(2, 6, 'Recommended by Charlie'),
(3, 3, 'Perfect for our workflow'),
(3, 8, NULL),
(3, 18, 'Try for sprint planning'),
(4, 2, NULL),
(4, 12, 'Best ML notebook'),
(4, 20, NULL),
(5, 4, NULL),
(5, 11, 'Evaluate for media project'),
(6, 7, 'Consider for backend refactor'),
(6, 15, NULL),
(7, 5, 'Security audit needed'),
(7, 9, NULL),
(8, 15, 'For edge monitoring'),
(8, 21, 'On-call improvements'),
(9, 10, 'Retail client interest'),
(9, 19, NULL),
(10, 26, 'Customer support improvement'),
(11, 13, NULL),
(11, 25, 'Inventory management'),
(12, 27, 'Fraud detection evaluation');

-- ==========================================
-- Warehouses
-- ==========================================
INSERT INTO warehouses (organization_id, code, name, address, city, country, latitude, longitude, capacity, manager_id, is_active) VALUES
(1, 'WH-SF-01', 'San Francisco Main', '100 Tech Street', 'San Francisco', 'United States', 37.7749, -122.4194, 10000, 1, true),
(1, 'WH-NY-01', 'New York Hub', '200 Commerce Ave', 'New York', 'United States', 40.7128, -74.0060, 8000, 2, true),
(1, 'WH-SEA-01', 'Seattle Fulfillment', '300 Cloud Drive', 'Seattle', 'United States', 47.6062, -122.3321, 12000, NULL, true),
(6, 'WH-CHI-01', 'Chicago Distribution', '500 Retail Lane', 'Chicago', 'United States', 41.8781, -87.6298, 15000, 7, true),
(6, 'WH-ATL-01', 'Atlanta Logistics', '600 Supply Chain Rd', 'Atlanta', 'United States', 33.7490, -84.3880, 20000, NULL, true),
(10, 'WH-ATL-02', 'Atlanta Express', '700 Fast Delivery Way', 'Atlanta', 'United States', 33.7550, -84.3900, 5000, 11, true),
(11, 'WH-SD-01', 'San Diego Medical', '800 Health Blvd', 'San Diego', 'United States', 32.7157, -117.1611, 6000, 12, true);

-- ==========================================
-- Stock Levels
-- ==========================================
INSERT INTO stock_levels (warehouse_id, product_id, quantity, reserved_quantity, reorder_point, last_counted_at) VALUES
(1, 1, 20, 5, 10, NOW() - INTERVAL '7 days'),
(1, 4, 15, 3, 5, NOW() - INTERVAL '7 days'),
(1, 5, 25, 2, 10, NOW() - INTERVAL '7 days'),
(1, 7, 18, 4, 8, NOW() - INTERVAL '7 days'),
(1, 17, 30, 5, 15, NOW() - INTERVAL '7 days'),
(2, 1, 15, 2, 10, NOW() - INTERVAL '5 days'),
(2, 4, 10, 1, 5, NOW() - INTERVAL '5 days'),
(2, 5, 20, 3, 10, NOW() - INTERVAL '5 days'),
(3, 1, 15, 0, 10, NOW() - INTERVAL '3 days'),
(3, 4, 5, 0, 5, NOW() - INTERVAL '3 days'),
(4, 10, 40, 8, 15, NOW() - INTERVAL '4 days'),
(4, 16, 45, 10, 20, NOW() - INTERVAL '4 days'),
(4, 25, 30, 5, 10, NOW() - INTERVAL '4 days'),
(5, 10, 25, 3, 10, NOW() - INTERVAL '6 days'),
(5, 25, 20, 2, 8, NOW() - INTERVAL '6 days'),
(5, 30, 15, 4, 5, NOW() - INTERVAL '6 days'),
(6, 13, 25, 3, 10, NOW() - INTERVAL '2 days'),
(6, 23, 50, 8, 20, NOW() - INTERVAL '2 days'),
(6, 30, 20, 5, 8, NOW() - INTERVAL '2 days'),
(7, 14, 25, 5, 10, NOW() - INTERVAL '1 day'),
(7, 28, 35, 8, 15, NOW() - INTERVAL '1 day');

-- ==========================================
-- Inventory Movements
-- ==========================================
INSERT INTO inventory_movements (warehouse_id, product_id, movement_type, quantity, reference_type, reference_id, notes, performed_by) VALUES
(1, 1, 'inbound', 50, 'purchase_order', 1001, 'Initial stock', 1),
(1, 1, 'outbound', 30, 'order', 1, 'Customer order', 1),
(1, 4, 'inbound', 30, 'purchase_order', 1002, 'Restock', 2),
(1, 4, 'outbound', 15, 'order', 2, 'Customer order', 2),
(2, 1, 'inbound', 30, 'transfer', NULL, 'Transfer from SF', 6),
(2, 1, 'outbound', 15, 'order', 3, 'Customer order', 6),
(4, 10, 'inbound', 60, 'purchase_order', 1003, 'Q4 stock', 7),
(4, 10, 'outbound', 20, 'order', 4, 'Bulk order', 7),
(6, 13, 'inbound', 40, 'purchase_order', 1004, 'New product launch', 11),
(6, 13, 'outbound', 15, 'order', 5, 'Customer order', 11),
(7, 14, 'inbound', 50, 'purchase_order', 1005, 'Medical supply', 12),
(7, 14, 'outbound', 25, 'order', 6, 'Hospital order', 12),
(1, 17, 'inbound', 40, 'purchase_order', 1006, 'Security product stock', 15),
(3, 1, 'inbound', 20, 'transfer', NULL, 'Transfer from NY', 8),
(5, 30, 'inbound', 25, 'purchase_order', 1007, 'Logistics product', 11);

-- ==========================================
-- Coupons
-- ==========================================
INSERT INTO marketing.coupons (code, name, description, discount_type, discount_value, min_order_amount, max_discount_amount, usage_limit, usage_count, usage_limit_per_user, starts_at, expires_at, is_active, created_by, organization_id) VALUES
('WELCOME20', 'Welcome 20% Off', 'New customer discount', 'percentage', 20.00, 100.00, 500.00, 1000, 245, 1, NOW() - INTERVAL '30 days', NOW() + INTERVAL '60 days', true, 1, 1),
('SUMMER2025', 'Summer Sale', 'Summer promotion discount', 'percentage', 15.00, 200.00, 300.00, 500, 89, 2, NOW() - INTERVAL '15 days', NOW() + INTERVAL '45 days', true, 2, 1),
('FLAT100', 'Flat $100 Off', 'Flat discount on orders over $500', 'fixed', 100.00, 500.00, NULL, 200, 34, 1, NOW() - INTERVAL '10 days', NOW() + INTERVAL '20 days', true, 4, 3),
('ENTERPRISE50', 'Enterprise Deal', 'Enterprise tier special discount', 'percentage', 25.00, 5000.00, 2000.00, 50, 12, 1, NOW() - INTERVAL '60 days', NOW() + INTERVAL '120 days', true, 5, 4),
('DESIGN15', 'Design Tools Discount', 'Discount on design products', 'percentage', 15.00, 200.00, 200.00, 300, 67, 1, NOW() - INTERVAL '20 days', NOW() + INTERVAL '40 days', true, 3, 2),
('FREESHIP', 'Free Shipping', 'Free shipping on all orders', 'fixed', 50.00, 100.00, 50.00, 1000, 432, 3, NOW() - INTERVAL '45 days', NOW() + INTERVAL '15 days', true, 9, 8),
('HEALTH25', 'Healthcare Discount', 'Healthcare organizations special', 'percentage', 25.00, 1000.00, 1000.00, 100, 23, 2, NOW() - INTERVAL '30 days', NOW() + INTERVAL '90 days', true, 12, 11),
('EXPIRED10', 'Expired Coupon', 'This coupon has expired', 'percentage', 10.00, 50.00, 100.00, 100, 45, 1, NOW() - INTERVAL '90 days', NOW() - INTERVAL '30 days', false, 1, 1);

-- ==========================================
-- Campaigns
-- ==========================================
INSERT INTO marketing.campaigns (organization_id, name, description, campaign_type, status, budget, spent, target_audience, starts_at, ends_at, created_by) VALUES
(1, 'Q4 Product Launch', 'New product line launch campaign', 'product_launch', 'active', 50000.00, 23450.00, '{"industries": ["technology", "finance"], "company_size": ["100-500", "500+"]}', NOW() - INTERVAL '30 days', NOW() + INTERVAL '60 days', 1),
(1, 'Developer Conference 2025', 'Annual developer conference promotion', 'event', 'active', 100000.00, 45000.00, '{"job_titles": ["developer", "engineer", "architect"]}', NOW() - INTERVAL '60 days', NOW() + INTERVAL '30 days', 2),
(2, 'Design Week Promo', 'Design week special offers', 'promotional', 'active', 15000.00, 8900.00, '{"industries": ["design", "creative"]}', NOW() - INTERVAL '10 days', NOW() + INTERVAL '20 days', 3),
(3, 'Analytics Webinar Series', 'Educational webinar campaign', 'content', 'completed', 20000.00, 18500.00, '{"job_titles": ["analyst", "data scientist", "manager"]}', NOW() - INTERVAL '90 days', NOW() - INTERVAL '30 days', 4),
(4, 'Cloud Migration Special', 'Cloud migration assistance offer', 'promotional', 'active', 75000.00, 32000.00, '{"company_size": ["100-500", "500+"], "needs": ["cloud migration"]}', NOW() - INTERVAL '45 days', NOW() + INTERVAL '45 days', 5),
(6, 'Retail Tech Summit', 'Retail technology summit sponsorship', 'event', 'draft', 30000.00, 0.00, '{"industries": ["retail", "e-commerce"]}', NOW() + INTERVAL '30 days', NOW() + INTERVAL '60 days', 7),
(11, 'Healthcare Compliance Month', 'Healthcare compliance awareness', 'awareness', 'active', 25000.00, 12300.00, '{"industries": ["healthcare", "pharma"]}', NOW() - INTERVAL '15 days', NOW() + INTERVAL '45 days', 12);

-- ==========================================
-- Promotions
-- ==========================================
INSERT INTO marketing.promotions (campaign_id, organization_id, name, promotion_type, discount_type, discount_value, buy_quantity, get_quantity, applicable_products, starts_at, ends_at, is_active, priority) VALUES
(1, 1, 'Launch Bundle Deal', 'bundle', 'percentage', 30.00, NULL, NULL, ARRAY[1, 4, 7], NOW() - INTERVAL '30 days', NOW() + INTERVAL '60 days', true, 10),
(1, 1, 'API Gateway Special', 'product_discount', 'percentage', 20.00, NULL, NULL, ARRAY[4], NOW() - INTERVAL '30 days', NOW() + INTERVAL '60 days', true, 5),
(2, 1, 'Conference Attendee Discount', 'coupon', 'percentage', 25.00, NULL, NULL, NULL, NOW() - INTERVAL '60 days', NOW() + INTERVAL '30 days', true, 15),
(3, 2, 'Design Tools BOGO', 'bogo', NULL, NULL, 2, 1, ARRAY[3, 8, 18], NOW() - INTERVAL '10 days', NOW() + INTERVAL '20 days', true, 10),
(5, 4, 'Cloud Storage Upgrade', 'product_discount', 'fixed', 200.00, NULL, NULL, ARRAY[1, 11], NOW() - INTERVAL '45 days', NOW() + INTERVAL '45 days', true, 5),
(7, 11, 'Healthcare Bundle', 'bundle', 'percentage', 20.00, NULL, NULL, ARRAY[14, 28], NOW() - INTERVAL '15 days', NOW() + INTERVAL '45 days', true, 10),
(NULL, 1, 'Flash Sale - Security', 'flash_sale', 'percentage', 35.00, NULL, NULL, ARRAY[5, 17], NOW(), NOW() + INTERVAL '24 hours', true, 20);

-- ==========================================
-- Shipping Zones
-- ==========================================
INSERT INTO sales.shipping_zones (organization_id, name, countries, is_active) VALUES
(1, 'US Domestic', ARRAY['US'], true),
(1, 'North America', ARRAY['US', 'CA', 'MX'], true),
(1, 'Europe', ARRAY['GB', 'DE', 'FR', 'IT', 'ES', 'NL', 'BE', 'AT', 'CH'], true),
(1, 'Asia Pacific', ARRAY['JP', 'KR', 'AU', 'NZ', 'SG', 'HK'], true),
(2, 'UK & Ireland', ARRAY['GB', 'IE'], true),
(2, 'Europe', ARRAY['DE', 'FR', 'IT', 'ES', 'NL', 'BE'], true),
(6, 'US Domestic', ARRAY['US'], true),
(6, 'Canada', ARRAY['CA'], true);

-- ==========================================
-- Shipping Rates
-- ==========================================
INSERT INTO sales.shipping_rates (zone_id, name, min_weight, max_weight, min_order_amount, max_order_amount, rate, estimated_days_min, estimated_days_max, is_active) VALUES
(1, 'Standard Shipping', NULL, NULL, 0, 100.00, 9.99, 5, 7, true),
(1, 'Express Shipping', NULL, NULL, 0, NULL, 19.99, 2, 3, true),
(1, 'Free Shipping', NULL, NULL, 100.00, NULL, 0.00, 5, 7, true),
(2, 'North America Standard', NULL, NULL, 0, NULL, 14.99, 7, 10, true),
(2, 'North America Express', NULL, NULL, 0, NULL, 29.99, 3, 5, true),
(3, 'Europe Standard', NULL, NULL, 0, NULL, 24.99, 10, 14, true),
(3, 'Europe Express', NULL, NULL, 0, NULL, 49.99, 5, 7, true),
(4, 'Asia Pacific Standard', NULL, NULL, 0, NULL, 29.99, 14, 21, true),
(4, 'Asia Pacific Express', NULL, NULL, 0, NULL, 59.99, 7, 10, true),
(5, 'UK Standard', NULL, NULL, 0, 50.00, 4.99, 2, 3, true),
(5, 'UK Free', NULL, NULL, 50.00, NULL, 0.00, 2, 3, true),
(7, 'Ground Shipping', 0, 10.0, NULL, NULL, 7.99, 5, 7, true),
(7, 'Heavy Freight', 10.0, 100.0, NULL, NULL, 24.99, 7, 10, true);

-- ==========================================
-- Orders
-- ==========================================
INSERT INTO sales.orders (order_number, user_id, organization_id, status, payment_status, shipping_status, total_amount, subtotal, tax_amount, discount_amount, coupon_id, billing_first_name, billing_last_name, billing_email, shipping_first_name, shipping_last_name, shipping_city, shipping_country, tracking_number, carrier, warehouse_id, estimated_delivery_date, created_at) VALUES
('ORD-2025-0001', 1, 1, 'completed', 'paid', 'delivered', 1099.99, 999.99, 100.00, 0.00, NULL, 'Alice', 'Johnson', 'alice@example.com', 'Alice', 'Johnson', 'San Francisco', 'United States', 'TRK-12345', 'FedEx', 1, NOW() + INTERVAL '7 days', NOW() - INTERVAL '10 days'),
('ORD-2025-0002', 2, 1, 'completed', 'paid', 'delivered', 2749.99, 2499.99, 250.00, 0.00, NULL, 'Bob', 'Smith', 'bob@example.com', 'Bob', 'Smith', 'New York', 'United States', 'TRK-12346', 'UPS', 2, NOW() + INTERVAL '5 days', NOW() - INTERVAL '8 days'),
('ORD-2025-0003', 3, 2, 'completed', 'paid', 'delivered', 659.99, 599.99, 60.00, 0.00, 5, 'Charlie', 'Brown', 'charlie@example.com', 'Charlie', 'Brown', 'London', 'United Kingdom', 'TRK-12347', 'DHL', NULL, NOW() + INTERVAL '10 days', NOW() - INTERVAL '15 days'),
('ORD-2025-0004', 4, 3, 'processing', 'paid', 'in_transit', 1319.99, 1199.99, 120.00, 0.00, NULL, 'Diana', 'Prince', 'diana@example.com', 'Diana', 'Prince', 'Austin', 'United States', 'TRK-12348', 'FedEx', 1, NOW() + INTERVAL '3 days', NOW() - INTERVAL '2 days'),
('ORD-2025-0005', 1, 2, 'pending', 'unpaid', 'unshipped', 549.99, 499.99, 50.00, 0.00, NULL, 'Alice', 'Johnson', 'alice@example.com', 'Alice', 'Johnson', 'San Francisco', 'United States', NULL, NULL, NULL, NOW() + INTERVAL '14 days', NOW() - INTERVAL '1 day'),
('ORD-2025-0006', 5, 1, 'completed', 'paid', 'delivered', 879.99, 799.99, 80.00, 0.00, 1, 'Eve', 'Wilson', 'eve@example.com', 'Eve', 'Wilson', 'Remote', 'United States', 'TRK-12349', 'UPS', 3, NOW() + INTERVAL '8 days', NOW() - INTERVAL '12 days'),
('ORD-2025-0007', 2, 3, 'completed', 'paid', 'delivered', 1649.99, 1499.99, 150.00, 0.00, NULL, 'Bob', 'Smith', 'bob@example.com', 'Bob', 'Smith', 'New York', 'United States', 'TRK-12350', 'FedEx', 2, NOW() + INTERVAL '6 days', NOW() - INTERVAL '9 days'),
('ORD-2025-0008', 3, 1, 'cancelled', 'refunded', 'returned', 1999.00, 1799.99, 199.01, 0.00, NULL, 'Charlie', 'Brown', 'charlie@example.com', 'Charlie', 'Brown', 'London', 'United Kingdom', NULL, NULL, NULL, NULL, NOW() - INTERVAL '20 days'),
('ORD-2025-0009', 6, 6, 'completed', 'paid', 'delivered', 769.99, 699.99, 70.00, 0.00, NULL, 'Frank', 'Li', 'frank.li@example.com', 'Frank', 'Li', 'Portland', 'United States', 'TRK-12351', 'UPS', 4, NOW() + INTERVAL '4 days', NOW() - INTERVAL '6 days'),
('ORD-2025-0010', 7, 1, 'processing', 'paid', 'in_transit', 1539.99, 1399.99, 140.00, 0.00, 2, 'Grace', 'Park', 'grace.park@example.com', 'Grace', 'Park', 'San Francisco', 'United States', 'TRK-12352', 'FedEx', 1, NOW() + INTERVAL '2 days', NOW() - INTERVAL '3 days'),
('ORD-2025-0011', 8, 4, 'completed', 'paid', 'delivered', 989.99, 899.99, 90.00, 0.00, NULL, 'Henry', 'Ng', 'henry.ng@example.com', 'Henry', 'Ng', 'Seattle', 'United States', 'TRK-12353', 'USPS', 3, NOW() + INTERVAL '5 days', NOW() - INTERVAL '7 days'),
('ORD-2025-0012', 9, 3, 'pending', 'unpaid', 'unshipped', 1099.99, 999.99, 100.00, 0.00, 3, 'Isabel', 'Chen', 'isabel.chen@example.com', 'Isabel', 'Chen', 'Austin', 'United States', NULL, NULL, NULL, NOW() + INTERVAL '12 days', NOW() - INTERVAL '1 day'),
('ORD-2025-0013', 10, 1, 'completed', 'paid', 'delivered', 439.99, 399.99, 40.00, 0.00, NULL, 'Jack', 'Turner', 'jack.turner@example.com', 'Jack', 'Turner', 'Denver', 'United States', 'TRK-12354', 'UPS', 1, NOW() + INTERVAL '6 days', NOW() - INTERVAL '9 days'),
('ORD-2025-0014', 11, 3, 'completed', 'paid', 'delivered', 2089.99, 1899.99, 190.00, 0.00, NULL, 'Kim', 'Min', 'kim.min@example.com', 'Kim', 'Min', 'Austin', 'United States', 'TRK-12355', 'DHL', 1, NOW() + INTERVAL '9 days', NOW() - INTERVAL '11 days'),
('ORD-2025-0015', 12, 4, 'processing', 'paid', 'in_transit', 769.99, 699.99, 70.00, 0.00, NULL, 'Leo', 'Martinez', 'leo.martinez@example.com', 'Leo', 'Martinez', 'Seattle', 'United States', 'TRK-12356', 'FedEx', 3, NOW() + INTERVAL '3 days', NOW() - INTERVAL '2 days'),
('ORD-2025-0016', 13, 2, 'completed', 'paid', 'delivered', 494.99, 449.99, 45.00, 0.00, 5, 'Maya', 'Patel', 'maya.patel@example.com', 'Maya', 'Patel', 'London', 'United Kingdom', 'TRK-12357', 'Royal Mail', NULL, NOW() + INTERVAL '7 days', NOW() - INTERVAL '8 days'),
('ORD-2025-0017', 14, 1, 'pending', 'unpaid', 'unshipped', 824.99, 749.99, 75.00, 0.00, NULL, 'Nora', 'Hughes', 'nora.hughes@example.com', 'Nora', 'Hughes', 'Chicago', 'United States', NULL, NULL, NULL, NOW() + INTERVAL '15 days', NOW() - INTERVAL '1 day'),
('ORD-2025-0018', 15, 1, 'completed', 'paid', 'delivered', 1099.99, 999.99, 100.00, 0.00, 1, 'Owen', 'Kim', 'owen.kim@example.com', 'Owen', 'Kim', 'Boston', 'United States', 'TRK-12358', 'FedEx', 2, NOW() + INTERVAL '4 days', NOW() - INTERVAL '5 days'),
('ORD-2025-0019', 6, 7, 'completed', 'paid', 'delivered', 1319.99, 1199.99, 120.00, 0.00, NULL, 'Frank', 'Li', 'frank.li@example.com', 'Frank', 'Li', 'Chicago', 'United States', 'TRK-12359', 'UPS', 4, NOW() + INTERVAL '6 days', NOW() - INTERVAL '10 days'),
('ORD-2025-0020', 7, 8, 'completed', 'paid', 'delivered', 1759.99, 1599.99, 160.00, 0.00, NULL, 'Grace', 'Park', 'grace.park@example.com', 'Grace', 'Park', 'Toronto', 'Canada', 'TRK-12360', 'Canada Post', NULL, NOW() + INTERVAL '8 days', NOW() - INTERVAL '12 days'),
('ORD-2025-0021', 8, 9, 'processing', 'paid', 'in_transit', 2419.99, 2199.99, 220.00, 0.00, NULL, 'Henry', 'Ng', 'henry.ng@example.com', 'Henry', 'Ng', 'Manchester', 'United Kingdom', 'TRK-12361', 'DHL', NULL, NOW() + INTERVAL '5 days', NOW() - INTERVAL '6 days'),
('ORD-2025-0022', 9, 10, 'completed', 'paid', 'delivered', 1539.99, 1399.99, 140.00, 0.00, NULL, 'Isabel', 'Chen', 'isabel.chen@example.com', 'Isabel', 'Chen', 'New York', 'United States', 'TRK-12362', 'FedEx', 2, NOW() + INTERVAL '7 days', NOW() - INTERVAL '9 days'),
('ORD-2025-0023', 10, 11, 'completed', 'paid', 'delivered', 1209.99, 1099.99, 110.00, 0.00, NULL, 'Jack', 'Turner', 'jack.turner@example.com', 'Jack', 'Turner', 'Atlanta', 'United States', 'TRK-12363', 'UPS', 6, NOW() + INTERVAL '6 days', NOW() - INTERVAL '8 days'),
('ORD-2025-0024', 11, 12, 'cancelled', 'refunded', 'returned', 2089.99, 1899.99, 190.00, 0.00, 7, 'Kim', 'Min', 'kim.min@example.com', 'Kim', 'Min', 'San Diego', 'United States', NULL, NULL, 7, NULL, NOW() - INTERVAL '18 days'),
('ORD-2025-0025', 16, 1, 'completed', 'paid', 'delivered', 1299.99, 1199.99, 100.00, 0.00, 2, 'Peter', 'Wang', 'peter.wang@example.com', 'Peter', 'Wang', 'Portland', 'United States', 'TRK-12364', 'FedEx', 1, NOW() + INTERVAL '5 days', NOW() - INTERVAL '7 days'),
('ORD-2025-0026', 17, 1, 'completed', 'paid', 'delivered', 549.99, 499.99, 50.00, 0.00, NULL, 'Quinn', 'Taylor', 'quinn.taylor@example.com', 'Quinn', 'Taylor', 'Austin', 'United States', 'TRK-12365', 'UPS', 1, NOW() + INTERVAL '4 days', NOW() - INTERVAL '6 days'),
('ORD-2025-0027', 18, 2, 'processing', 'paid', 'in_transit', 659.99, 599.99, 60.00, 0.00, 5, 'Rachel', 'Green', 'rachel.green@example.com', 'Rachel', 'Green', 'London', 'United Kingdom', 'TRK-12366', 'Royal Mail', NULL, NOW() + INTERVAL '3 days', NOW() - INTERVAL '2 days'),
('ORD-2025-0028', 19, 4, 'completed', 'paid', 'delivered', 1649.99, 1499.99, 150.00, 0.00, 4, 'Sam', 'Wilson', 'sam.wilson@example.com', 'Sam', 'Wilson', 'Seattle', 'United States', 'TRK-12367', 'FedEx', 3, NOW() + INTERVAL '6 days', NOW() - INTERVAL '9 days'),
('ORD-2025-0029', 20, 3, 'completed', 'paid', 'delivered', 1759.99, 1599.99, 160.00, 0.00, NULL, 'Tina', 'Rodriguez', 'tina.rodriguez@example.com', 'Tina', 'Rodriguez', 'Miami', 'United States', 'TRK-12368', 'UPS', 1, NOW() + INTERVAL '5 days', NOW() - INTERVAL '8 days'),
('ORD-2025-0030', 1, 4, 'pending', 'pending', 'unshipped', 2749.99, 2499.99, 250.00, 0.00, 4, 'Alice', 'Johnson', 'alice@example.com', 'Alice', 'Johnson', 'San Francisco', 'United States', NULL, NULL, 3, NOW() + INTERVAL '10 days', NOW() - INTERVAL '1 hour');

-- ==========================================
-- Order Items
-- ==========================================
INSERT INTO sales.order_items (order_id, product_id, quantity, unit_price, line_total) VALUES
(1, 1, 1, 999.99, 999.99),
(2, 4, 1, 2499.99, 2499.99),
(3, 3, 1, 599.99, 599.99),
(4, 2, 1, 1199.99, 1199.99),
(5, 8, 1, 499.99, 499.99),
(6, 5, 1, 799.99, 799.99),
(7, 2, 1, 1499.99, 1499.99),
(8, 7, 1, 1799.99, 1799.99),
(9, 16, 1, 699.99, 699.99),
(10, 15, 1, 1399.99, 1399.99),
(11, 9, 1, 899.99, 899.99),
(12, 1, 1, 999.99, 999.99),
(13, 6, 1, 399.99, 399.99),
(14, 14, 1, 1899.99, 1899.99),
(15, 8, 1, 499.99, 499.99),
(16, 18, 1, 449.99, 449.99),
(17, 21, 1, 749.99, 749.99),
(18, 17, 1, 999.99, 999.99),
(19, 10, 1, 1299.99, 1299.99),
(20, 12, 1, 1599.99, 1599.99),
(21, 11, 1, 2199.99, 2199.99),
(22, 22, 1, 1399.99, 1399.99),
(23, 23, 1, 899.99, 899.99),
(24, 20, 1, 1999.99, 1999.99),
(10, 26, 1, 899.99, 899.99),
(12, 25, 1, 1099.99, 1099.99),
(13, 29, 1, 599.99, 599.99),
(16, 18, 2, 449.99, 899.98),
(19, 27, 1, 1699.99, 1699.99),
(20, 28, 1, 1299.99, 1299.99),
(21, 30, 1, 1899.99, 1899.99),
(22, 23, 2, 899.99, 1799.98),
(25, 2, 1, 1199.99, 1199.99),
(26, 8, 1, 499.99, 499.99),
(27, 3, 1, 599.99, 599.99),
(28, 15, 1, 1399.99, 1399.99),
(29, 12, 1, 1599.99, 1599.99),
(30, 4, 1, 2499.99, 2499.99);

-- ==========================================
-- Payments
-- ==========================================
INSERT INTO sales.payments (order_id, amount, payment_method, payment_status, transaction_id, gateway_response_code, authorization_code, cvv_verified, risk_score) VALUES
(1, 1099.99, 'credit_card', 'completed', 'TXN-001-2025', '00', 'AUTH-001', true, 5.2),
(2, 2749.99, 'credit_card', 'completed', 'TXN-002-2025', '00', 'AUTH-002', true, 3.8),
(3, 659.99, 'credit_card', 'completed', 'TXN-003-2025', '00', 'AUTH-003', true, 4.1),
(4, 1319.99, 'credit_card', 'completed', 'TXN-004-2025', '00', 'AUTH-004', true, 6.7),
(5, 549.99, 'bank_transfer', 'pending', NULL, NULL, NULL, NULL, 0.0),
(6, 879.99, 'credit_card', 'completed', 'TXN-006-2025', '00', 'AUTH-006', true, 4.5),
(7, 1649.99, 'credit_card', 'completed', 'TXN-007-2025', '00', 'AUTH-007', true, 3.2),
(8, 1999.00, 'credit_card', 'refunded', 'TXN-008-2025', '00', 'AUTH-008', true, 12.3),
(9, 769.99, 'credit_card', 'completed', 'TXN-009-2025', '00', 'AUTH-009', true, 3.7),
(10, 1539.99, 'credit_card', 'completed', 'TXN-010-2025', '00', 'AUTH-010', true, 4.2),
(11, 989.99, 'credit_card', 'completed', 'TXN-011-2025', '00', 'AUTH-011', true, 5.1),
(12, 1099.99, 'bank_transfer', 'pending', NULL, NULL, NULL, NULL, 0.0),
(13, 439.99, 'credit_card', 'completed', 'TXN-013-2025', '00', 'AUTH-013', true, 2.9),
(14, 2089.99, 'credit_card', 'completed', 'TXN-014-2025', '00', 'AUTH-014', true, 6.0),
(15, 769.99, 'credit_card', 'completed', 'TXN-015-2025', '00', 'AUTH-015', true, 3.4),
(16, 494.99, 'credit_card', 'completed', 'TXN-016-2025', '00', 'AUTH-016', true, 4.8),
(17, 824.99, 'bank_transfer', 'pending', NULL, NULL, NULL, NULL, 0.0),
(18, 1099.99, 'credit_card', 'completed', 'TXN-018-2025', '00', 'AUTH-018', true, 5.5),
(19, 1319.99, 'credit_card', 'completed', 'TXN-019-2025', '00', 'AUTH-019', true, 4.1),
(20, 1759.99, 'credit_card', 'completed', 'TXN-020-2025', '00', 'AUTH-020', true, 3.6),
(21, 2419.99, 'credit_card', 'completed', 'TXN-021-2025', '00', 'AUTH-021', true, 6.3),
(22, 1539.99, 'credit_card', 'completed', 'TXN-022-2025', '00', 'AUTH-022', true, 4.0),
(23, 1209.99, 'credit_card', 'completed', 'TXN-023-2025', '00', 'AUTH-023', true, 3.9),
(24, 2089.99, 'credit_card', 'refunded', 'TXN-024-2025', '00', 'AUTH-024', true, 11.2),
(25, 1299.99, 'credit_card', 'completed', 'TXN-025-2025', '00', 'AUTH-025', true, 4.3),
(26, 549.99, 'credit_card', 'completed', 'TXN-026-2025', '00', 'AUTH-026', true, 3.1),
(27, 659.99, 'credit_card', 'completed', 'TXN-027-2025', '00', 'AUTH-027', true, 4.7),
(28, 1649.99, 'credit_card', 'completed', 'TXN-028-2025', '00', 'AUTH-028', true, 5.0),
(29, 1759.99, 'credit_card', 'completed', 'TXN-029-2025', '00', 'AUTH-029', true, 3.8),
(30, 2749.99, 'credit_card', 'pending', NULL, NULL, NULL, NULL, 0.0);

-- ==========================================
-- Coupon Usages
-- ==========================================
INSERT INTO marketing.coupon_usages (coupon_id, user_id, order_id, discount_applied) VALUES
(1, 5, 6, 160.00),
(1, 15, 18, 200.00),
(2, 7, 10, 210.00),
(2, 16, 25, 180.00),
(3, 9, 12, 100.00),
(4, 19, 28, 375.00),
(5, 3, 3, 90.00),
(5, 13, 16, 67.50),
(5, 18, 27, 90.00),
(7, 11, 24, 475.00);

-- ==========================================
-- Reviews
-- ==========================================
INSERT INTO reviews (product_id, user_id, order_id, rating, title, content, pros, cons, is_verified_purchase, helpful_count, not_helpful_count, status, moderated_by, moderated_at) VALUES
(1, 1, 1, 5, 'Excellent cloud storage!', 'This has transformed how our team handles data storage. Fast, reliable, and secure.', 'Fast uploads, great security, excellent uptime', 'Could use more integrations', true, 45, 2, 'approved', 2, NOW() - INTERVAL '9 days'),
(2, 4, 4, 5, 'Best analytics dashboard', 'Real-time insights have helped us make better decisions faster.', 'Real-time data, beautiful UI, easy to use', 'Learning curve for advanced features', true, 78, 3, 'approved', 1, NOW() - INTERVAL '1 day'),
(3, 3, 3, 4, 'Great for designers', 'Comprehensive tool suite for professional design work.', 'Wide range of tools, good collaboration', 'Could be faster on complex projects', true, 23, 5, 'approved', 1, NOW() - INTERVAL '14 days'),
(4, 2, 2, 5, 'Powerful API management', 'Handles all our API needs with excellent performance.', 'Scalable, great documentation, reliable', 'Premium pricing', true, 56, 1, 'approved', 1, NOW() - INTERVAL '7 days'),
(5, 5, 6, 4, 'Solid security suite', 'Comprehensive security tools that give us peace of mind.', 'Complete coverage, easy setup', 'UI could be more intuitive', true, 34, 4, 'approved', 2, NOW() - INTERVAL '11 days'),
(6, 10, 13, 5, 'Perfect for mobile dev', 'Streamlined our mobile development workflow significantly.', 'Great tooling, cross-platform support', 'Documentation could be better', true, 42, 2, 'approved', 1, NOW() - INTERVAL '8 days'),
(16, 6, 9, 4, 'Good team workspace', 'Has improved our team collaboration noticeably.', 'Easy sharing, good permissions', 'Needs more third-party integrations', true, 28, 6, 'approved', 2, NOW() - INTERVAL '5 days'),
(15, 7, 10, 4, 'Useful edge monitoring', 'Helps us keep track of edge devices effectively.', 'Real-time alerts, good coverage', 'Can be expensive at scale', true, 19, 3, 'approved', 1, NOW() - INTERVAL '2 days'),
(12, 7, 20, 5, 'Amazing for ML teams', 'The best notebook tool for data science collaboration.', 'Collaborative, powerful compute, versioning', 'Can be slow with very large datasets', true, 67, 4, 'approved', 4, NOW() - INTERVAL '11 days'),
(14, 11, 14, 5, 'HIPAA compliance done right', 'Finally a storage solution that meets all our healthcare compliance needs.', 'HIPAA compliant, secure, reliable', 'Premium pricing for healthcare features', true, 89, 2, 'approved', 12, NOW() - INTERVAL '10 days'),
(17, 15, 18, 5, 'Essential for API security', 'Caught several potential security issues before they became problems.', 'Real-time threat detection, easy setup', 'Could have more detailed reports', true, 52, 3, 'approved', 1, NOW() - INTERVAL '4 days'),
(2, 20, 29, 4, 'Very useful analytics', 'Great tool for understanding our business metrics.', 'Good visualizations, real-time updates', 'Takes time to set up properly', true, 31, 5, 'approved', 4, NOW() - INTERVAL '7 days'),
(8, 18, 27, 4, 'Solid content management', 'Handles our content workflow very well.', 'Good organization, team features', 'Search could be improved', true, 22, 4, 'approved', 3, NOW() - INTERVAL '1 day'),
(10, 6, 19, 5, 'Retail insights that matter', 'Transformed our understanding of customer behavior.', 'Accurate data, actionable insights', 'Could use more retail-specific features', true, 38, 2, 'approved', 7, NOW() - INTERVAL '9 days'),
(23, 10, 23, 4, 'Alerting done right', 'Low latency alerts have prevented several outages.', 'Fast alerts, flexible rules', 'UI could be more modern', true, 27, 3, 'approved', 11, NOW() - INTERVAL '7 days'),
(4, 19, 28, 5, 'Enterprise-grade API gateway', 'Handles our massive API traffic without breaking a sweat.', 'Highly scalable, excellent support', 'Initial setup complexity', true, 63, 2, 'approved', 5, NOW() - INTERVAL '8 days'),
(1, 16, 25, 5, 'Reliable storage solution', 'Never had any data loss or downtime issues.', 'Reliable, fast, secure', 'Could use more admin features', true, 35, 3, 'pending', NULL, NULL),
(3, 13, 16, 4, 'Good design tools', 'Helpful for our design team workflow.', 'Good features, nice templates', 'Could integrate better with Figma', true, 18, 4, 'pending', NULL, NULL);

-- ==========================================
-- Review Votes
-- ==========================================
INSERT INTO review_votes (review_id, user_id, is_helpful) VALUES
(1, 2, true), (1, 3, true), (1, 4, true), (1, 5, true), (1, 6, false),
(2, 1, true), (2, 3, true), (2, 5, true), (2, 6, true), (2, 7, true),
(3, 1, true), (3, 4, true), (3, 5, false), (3, 6, true),
(4, 1, true), (4, 3, true), (4, 4, true), (4, 5, true),
(5, 1, true), (5, 2, true), (5, 3, false), (5, 4, true),
(6, 1, true), (6, 2, true), (6, 3, true), (6, 4, true),
(7, 1, true), (7, 2, false), (7, 3, true), (7, 8, false),
(8, 1, true), (8, 2, true), (8, 8, false),
(9, 1, true), (9, 2, true), (9, 3, true), (9, 4, true), (9, 8, true),
(10, 1, true), (10, 2, true), (10, 3, true), (10, 4, true), (10, 5, true), (10, 11, true);

-- ==========================================
-- Notifications
-- ==========================================
INSERT INTO notifications (user_id, type, title, message, data, is_read, read_at, action_url) VALUES
(1, 'order_shipped', 'Your order has shipped!', 'Order ORD-2025-0001 is on its way.', '{"order_id": 1, "tracking": "TRK-12345"}', true, NOW() - INTERVAL '9 days', '/orders/1'),
(1, 'review_approved', 'Your review was approved', 'Your review for Cloud Storage Pro has been published.', '{"review_id": 1, "product_id": 1}', true, NOW() - INTERVAL '8 days', '/products/1'),
(2, 'order_shipped', 'Your order has shipped!', 'Order ORD-2025-0002 is on its way.', '{"order_id": 2, "tracking": "TRK-12346"}', true, NOW() - INTERVAL '7 days', '/orders/2'),
(3, 'promotion', 'Design Week Special!', 'Get 15% off all design tools this week.', '{"coupon_code": "DESIGN15"}', false, NULL, '/promotions/design-week'),
(4, 'order_processing', 'Order is being processed', 'Order ORD-2025-0004 is being prepared.', '{"order_id": 4}', true, NOW() - INTERVAL '1 day', '/orders/4'),
(5, 'welcome', 'Welcome to our platform!', 'Thanks for signing up. Here is your welcome discount.', '{"coupon_code": "WELCOME20"}', false, NULL, '/account'),
(6, 'order_delivered', 'Order delivered!', 'Order ORD-2025-0009 has been delivered.', '{"order_id": 9}', true, NOW() - INTERVAL '5 days', '/orders/9'),
(7, 'review_helpful', 'Your review helped others!', '5 people found your review helpful.', '{"review_id": 7}', false, NULL, '/reviews/7'),
(8, 'system', 'System maintenance scheduled', 'Scheduled maintenance on Dec 15, 2025 from 2-4 AM UTC.', '{"maintenance_id": 123}', false, NULL, '/status'),
(9, 'order_pending', 'Payment pending', 'Please complete payment for order ORD-2025-0012.', '{"order_id": 12}', false, NULL, '/orders/12/payment'),
(10, 'order_delivered', 'Order delivered!', 'Order ORD-2025-0023 has been delivered.', '{"order_id": 23}', true, NOW() - INTERVAL '7 days', '/orders/23'),
(11, 'refund_processed', 'Refund processed', 'Your refund for order ORD-2025-0024 has been processed.', '{"order_id": 24, "amount": 2089.99}', true, NOW() - INTERVAL '17 days', '/orders/24'),
(12, 'campaign_live', 'Campaign is now live!', 'Healthcare Compliance Month campaign has started.', '{"campaign_id": 7}', true, NOW() - INTERVAL '14 days', '/campaigns/7'),
(1, 'new_feature', 'New feature available!', 'Check out our new AI-powered analytics.', '{"feature": "ai_analytics"}', false, NULL, '/features/ai-analytics'),
(2, 'security', 'New login detected', 'New login from New York, NY on Chrome browser.', '{"ip": "192.168.1.101", "location": "New York, NY"}', true, NOW() - INTERVAL '1 day', '/security/sessions'),
(15, 'review_approved', 'Your review was approved', 'Your review for API Shield has been published.', '{"review_id": 11, "product_id": 17}', true, NOW() - INTERVAL '3 days', '/products/17');

-- ==========================================
-- Notification Preferences
-- ==========================================
INSERT INTO notification_preferences (user_id, notification_type, email_enabled, push_enabled, sms_enabled, in_app_enabled) VALUES
(1, 'order_updates', true, true, true, true),
(1, 'marketing', true, false, false, true),
(1, 'security', true, true, true, true),
(1, 'reviews', true, true, false, true),
(2, 'order_updates', true, true, false, true),
(2, 'marketing', false, false, false, true),
(2, 'security', true, true, true, true),
(3, 'order_updates', true, true, false, true),
(3, 'marketing', true, true, false, true),
(4, 'order_updates', true, false, false, true),
(4, 'marketing', false, false, false, false),
(5, 'order_updates', true, true, false, true),
(6, 'order_updates', true, true, false, true),
(6, 'marketing', true, false, false, true),
(7, 'order_updates', true, true, true, true),
(7, 'security', true, true, true, true),
(8, 'order_updates', true, false, false, true),
(9, 'order_updates', true, true, false, true),
(9, 'marketing', true, true, false, true),
(10, 'order_updates', true, true, false, true),
(11, 'order_updates', true, false, false, true),
(12, 'marketing', true, true, false, true),
(12, 'campaigns', true, true, false, true);

-- ==========================================
-- Analytics Events
-- ==========================================
INSERT INTO analytics.events (event_type, user_id, session_id, organization_id, properties, page_url, referrer, user_agent, ip_address, country, city, device_type, browser, os) VALUES
('page_view', 1, 'a1b2c3d4-e5f6-7890-abcd-ef1234567890', 1, '{"page": "dashboard"}', '/dashboard', 'https://google.com', 'Mozilla/5.0 Chrome/120', '192.168.1.100', 'US', 'San Francisco', 'desktop', 'Chrome', 'macOS'),
('product_view', 1, 'a1b2c3d4-e5f6-7890-abcd-ef1234567890', 1, '{"product_id": 1, "product_name": "Cloud Storage Pro"}', '/products/1', '/products', 'Mozilla/5.0 Chrome/120', '192.168.1.100', 'US', 'San Francisco', 'desktop', 'Chrome', 'macOS'),
('add_to_cart', 1, 'a1b2c3d4-e5f6-7890-abcd-ef1234567890', 1, '{"product_id": 1, "quantity": 1}', '/cart', '/products/1', 'Mozilla/5.0 Chrome/120', '192.168.1.100', 'US', 'San Francisco', 'desktop', 'Chrome', 'macOS'),
('checkout_start', 1, 'a1b2c3d4-e5f6-7890-abcd-ef1234567890', 1, '{"cart_value": 999.99}', '/checkout', '/cart', 'Mozilla/5.0 Chrome/120', '192.168.1.100', 'US', 'San Francisco', 'desktop', 'Chrome', 'macOS'),
('purchase', 1, 'a1b2c3d4-e5f6-7890-abcd-ef1234567890', 1, '{"order_id": 1, "total": 1099.99}', '/checkout/success', '/checkout', 'Mozilla/5.0 Chrome/120', '192.168.1.100', 'US', 'San Francisco', 'desktop', 'Chrome', 'macOS'),
('page_view', 2, 'b2c3d4e5-f6a7-8901-bcde-f12345678901', 1, '{"page": "products"}', '/products', 'https://bing.com', 'Mozilla/5.0 Firefox/121', '192.168.1.101', 'US', 'New York', 'desktop', 'Firefox', 'Windows'),
('search', 2, 'b2c3d4e5-f6a7-8901-bcde-f12345678901', 1, '{"query": "api gateway", "results_count": 5}', '/search', '/products', 'Mozilla/5.0 Firefox/121', '192.168.1.101', 'US', 'New York', 'desktop', 'Firefox', 'Windows'),
('product_view', 2, 'b2c3d4e5-f6a7-8901-bcde-f12345678901', 1, '{"product_id": 4, "product_name": "API Gateway"}', '/products/4', '/search', 'Mozilla/5.0 Firefox/121', '192.168.1.101', 'US', 'New York', 'desktop', 'Firefox', 'Windows'),
('page_view', 3, 'c3d4e5f6-a7b8-9012-cdef-123456789012', 2, '{"page": "dashboard"}', '/dashboard', NULL, 'Safari/17.0', '172.16.0.25', 'GB', 'London', 'desktop', 'Safari', 'macOS'),
('feature_used', 3, 'c3d4e5f6-a7b8-9012-cdef-123456789012', 2, '{"feature": "design_export", "format": "png"}', '/design/export', '/design', 'Safari/17.0', '172.16.0.25', 'GB', 'London', 'desktop', 'Safari', 'macOS'),
('page_view', NULL, 'd4e5f6a7-b8c9-0123-defa-234567890123', NULL, '{"page": "pricing"}', '/pricing', 'https://google.com', 'Mozilla/5.0 Chrome/120', '10.0.0.50', 'US', 'Austin', 'mobile', 'Chrome', 'Android'),
('signup_start', NULL, 'd4e5f6a7-b8c9-0123-defa-234567890123', NULL, '{}', '/signup', '/pricing', 'Mozilla/5.0 Chrome/120', '10.0.0.50', 'US', 'Austin', 'mobile', 'Chrome', 'Android'),
('page_view', 4, 'e5f6a7b8-c9d0-1234-efab-345678901234', 3, '{"page": "analytics"}', '/analytics', NULL, 'Edge/120', '192.168.2.50', 'US', 'Austin', 'desktop', 'Edge', 'Windows'),
('report_generated', 4, 'e5f6a7b8-c9d0-1234-efab-345678901234', 3, '{"report_type": "monthly_summary", "format": "pdf"}', '/analytics/reports', '/analytics', 'Edge/120', '192.168.2.50', 'US', 'Austin', 'desktop', 'Edge', 'Windows'),
('api_call', 1, NULL, 1, '{"endpoint": "/api/v1/products", "method": "GET", "status": 200}', NULL, NULL, 'API Client/1.0', '192.168.1.100', 'US', 'San Francisco', NULL, NULL, NULL),
('api_call', 1, NULL, 1, '{"endpoint": "/api/v1/orders", "method": "POST", "status": 201}', NULL, NULL, 'API Client/1.0', '192.168.1.100', 'US', 'San Francisco', NULL, NULL, NULL),
('login', 5, 'f6a7b8c9-d0e1-2345-fabc-456789012345', 4, '{"method": "password"}', '/login', NULL, 'Safari/17.0', '10.10.10.100', 'US', 'Seattle', 'desktop', 'Safari', 'macOS'),
('login', 6, 'a7b8c9d0-e1f2-3456-abcd-567890123456', 4, '{"method": "sso"}', '/login/sso', '/login', 'Chrome/120', '192.168.1.150', 'US', 'Portland', 'desktop', 'Chrome', 'Linux'),
('error', NULL, 'b8c9d0e1-f2a3-4567-bcde-678901234567', NULL, '{"error_type": "404", "url": "/nonexistent"}', '/nonexistent', '/products', 'Mozilla/5.0 Chrome/120', '10.0.0.100', 'JP', 'Tokyo', 'mobile', 'Chrome', 'iOS'),
('feedback', 7, 'c9d0e1f2-a3b4-5678-cdef-789012345678', 1, '{"rating": 5, "comment": "Great product!"}', '/feedback', '/dashboard', 'Safari/17.0 Mobile', '172.20.0.75', 'CA', 'Toronto', 'tablet', 'Safari', 'iPadOS');

-- ==========================================
-- Daily Metrics
-- ==========================================
INSERT INTO analytics.daily_metrics (organization_id, metric_date, metric_name, metric_value, dimensions) VALUES
(1, CURRENT_DATE - INTERVAL '7 days', 'daily_revenue', 15234.50, '{"currency": "USD"}'),
(1, CURRENT_DATE - INTERVAL '6 days', 'daily_revenue', 18456.75, '{"currency": "USD"}'),
(1, CURRENT_DATE - INTERVAL '5 days', 'daily_revenue', 12890.00, '{"currency": "USD"}'),
(1, CURRENT_DATE - INTERVAL '4 days', 'daily_revenue', 21345.25, '{"currency": "USD"}'),
(1, CURRENT_DATE - INTERVAL '3 days', 'daily_revenue', 19876.80, '{"currency": "USD"}'),
(1, CURRENT_DATE - INTERVAL '2 days', 'daily_revenue', 16543.00, '{"currency": "USD"}'),
(1, CURRENT_DATE - INTERVAL '1 day', 'daily_revenue', 22100.50, '{"currency": "USD"}'),
(1, CURRENT_DATE - INTERVAL '7 days', 'daily_orders', 15, NULL),
(1, CURRENT_DATE - INTERVAL '6 days', 'daily_orders', 18, NULL),
(1, CURRENT_DATE - INTERVAL '5 days', 'daily_orders', 12, NULL),
(1, CURRENT_DATE - INTERVAL '4 days', 'daily_orders', 21, NULL),
(1, CURRENT_DATE - INTERVAL '3 days', 'daily_orders', 19, NULL),
(1, CURRENT_DATE - INTERVAL '2 days', 'daily_orders', 16, NULL),
(1, CURRENT_DATE - INTERVAL '1 day', 'daily_orders', 22, NULL),
(1, CURRENT_DATE - INTERVAL '7 days', 'active_users', 234, NULL),
(1, CURRENT_DATE - INTERVAL '6 days', 'active_users', 256, NULL),
(1, CURRENT_DATE - INTERVAL '5 days', 'active_users', 198, NULL),
(1, CURRENT_DATE - INTERVAL '4 days', 'active_users', 278, NULL),
(1, CURRENT_DATE - INTERVAL '3 days', 'active_users', 312, NULL),
(1, CURRENT_DATE - INTERVAL '2 days', 'active_users', 287, NULL),
(1, CURRENT_DATE - INTERVAL '1 day', 'active_users', 345, NULL),
(2, CURRENT_DATE - INTERVAL '7 days', 'daily_revenue', 4567.80, '{"currency": "GBP"}'),
(2, CURRENT_DATE - INTERVAL '6 days', 'daily_revenue', 5234.50, '{"currency": "GBP"}'),
(2, CURRENT_DATE - INTERVAL '5 days', 'daily_revenue', 3890.00, '{"currency": "GBP"}'),
(3, CURRENT_DATE - INTERVAL '7 days', 'daily_revenue', 8765.40, '{"currency": "USD"}'),
(3, CURRENT_DATE - INTERVAL '6 days', 'daily_revenue', 9234.80, '{"currency": "USD"}'),
(3, CURRENT_DATE - INTERVAL '5 days', 'daily_revenue', 7654.20, '{"currency": "USD"}'),
(4, CURRENT_DATE - INTERVAL '7 days', 'api_calls', 1234567, '{"tier": "enterprise"}'),
(4, CURRENT_DATE - INTERVAL '6 days', 'api_calls', 1456789, '{"tier": "enterprise"}'),
(4, CURRENT_DATE - INTERVAL '5 days', 'api_calls', 1345678, '{"tier": "enterprise"}');

-- ==========================================
-- Audit Logs
-- ==========================================
INSERT INTO audit_logs (user_id, organization_id, table_schema, table_name, operation, record_id, old_values, new_values, changed_fields, ip_address, request_id) VALUES
(1, 1, 'public', 'users', 'INSERT', 1, NULL, '{"username":"alice","email":"alice@example.com"}', ARRAY['username', 'email'], '192.168.1.100', 'a0000001-0001-4000-8000-000000002025'),
(2, 1, 'public', 'users', 'INSERT', 2, NULL, '{"username":"bob","email":"bob@example.com"}', ARRAY['username', 'email'], '192.168.1.101', 'a0000002-0002-4000-8000-000000002025'),
(1, 1, 'public', 'organizations', 'INSERT', 1, NULL, '{"name":"Tech Corp","slug":"tech-corp"}', ARRAY['name', 'slug'], '192.168.1.100', 'a0000003-0003-4000-8000-000000002025'),
(1, 1, 'sales', 'orders', 'INSERT', 1, NULL, '{"order_number":"ORD-2025-0001","status":"pending"}', ARRAY['order_number', 'status'], '192.168.1.100', 'a0000004-0004-4000-8000-000000002025'),
(1, 1, 'sales', 'orders', 'UPDATE', 1, '{"status":"pending"}', '{"status":"completed","payment_status":"paid"}', ARRAY['status', 'payment_status'], '192.168.1.100', 'a0000005-0005-4000-8000-000000002025'),
(4, 3, 'public', 'products', 'INSERT', 1, NULL, '{"sku":"PROD-001","name":"Cloud Storage Pro"}', ARRAY['sku', 'name'], '192.168.2.50', 'a0000006-0006-4000-8000-000000002025'),
(2, 1, 'sales', 'orders', 'INSERT', 2, NULL, '{"order_number":"ORD-2025-0002","status":"pending"}', ARRAY['order_number', 'status'], '192.168.1.101', 'a0000007-0007-4000-8000-000000002025'),
(1, 1, 'sales', 'payments', 'INSERT', 1, NULL, '{"transaction_id":"TXN-001-2025","payment_status":"completed"}', ARRAY['transaction_id', 'payment_status'], '192.168.1.100', 'a0000008-0008-4000-8000-000000002025'),
(6, 5, 'public', 'users', 'INSERT', 6, NULL, '{"username":"frank","email":"frank.li@example.com"}', ARRAY['username', 'email'], '192.168.1.150', 'a0000009-0009-4000-8000-000000002025'),
(7, 6, 'public', 'users', 'INSERT', 7, NULL, '{"username":"grace","email":"grace.park@example.com"}', ARRAY['username', 'email'], '172.20.0.75', 'a0000010-0010-4000-8000-000000002025'),
(8, 7, 'public', 'organizations', 'INSERT', 5, NULL, '{"name":"Greenfield Ops","slug":"greenfield-ops"}', ARRAY['name', 'slug'], '192.168.1.160', 'a0000011-0011-4000-8000-000000002025'),
(9, 8, 'public', 'organizations', 'INSERT', 6, NULL, '{"name":"Northwind Retail","slug":"northwind-retail"}', ARRAY['name', 'slug'], '10.0.0.60', 'a0000012-0012-4000-8000-000000002025'),
(1, 1, 'public', 'products', 'UPDATE', 1, '{"price": 899.99}', '{"price": 999.99}', ARRAY['price'], '192.168.1.100', 'a0000013-0013-4000-8000-000000002025'),
(2, 1, 'public', 'organization_members', 'INSERT', NULL, NULL, '{"user_id": 7, "role": "member"}', ARRAY['user_id', 'role'], '192.168.1.101', 'a0000014-0014-4000-8000-000000002025'),
(5, 4, 'marketing', 'coupons', 'INSERT', 4, NULL, '{"code": "ENTERPRISE50", "discount_value": 25.00}', ARRAY['code', 'discount_value'], '10.10.10.100', 'a0000015-0015-4000-8000-000000002025'),
(12, 11, 'marketing', 'campaigns', 'UPDATE', 7, '{"status": "draft"}', '{"status": "active"}', ARRAY['status'], '192.168.3.100', 'a0000016-0016-4000-8000-000000002025'),
(1, 1, 'public', 'users', 'UPDATE', 1, '{"last_login_at": null}', '{"last_login_at": "2025-01-15T10:30:00Z"}', ARRAY['last_login_at'], '192.168.1.100', 'a0000017-0017-4000-8000-000000002025'),
(3, 2, 'public', 'categories', 'INSERT', 1, NULL, '{"slug": "software", "name": "Software"}', ARRAY['slug', 'name'], '172.16.0.25', 'a0000018-0018-4000-8000-000000002025'),
(4, 3, 'public', 'tags', 'INSERT', 1, NULL, '{"slug": "enterprise", "name": "Enterprise"}', ARRAY['slug', 'name'], '192.168.2.50', 'a0000019-0019-4000-8000-000000002025'),
(1, 1, 'analytics', 'events', 'INSERT', 1, NULL, '{"event_type": "page_view"}', ARRAY['event_type'], '192.168.1.100', 'a0000020-0020-4000-8000-000000002025');

-- ==========================================
-- Settings
-- ==========================================
INSERT INTO settings (organization_id, setting_key, setting_value, data_type, is_public, is_encrypted) VALUES
(1, 'theme_color', '#0066CC', 'string', true, false),
(1, 'max_api_calls', '10000', 'integer', false, false),
(1, 'email_notifications_enabled', 'true', 'boolean', false, false),
(1, 'timezone', 'America/Los_Angeles', 'string', false, false),
(1, 'session_timeout_minutes', '30', 'integer', false, false),
(1, 'two_factor_required', 'false', 'boolean', false, false),
(1, 'api_rate_limit', '1000', 'integer', false, false),
(1, 'webhook_secret', 'encrypted_secret_1', 'string', false, true),
(2, 'theme_color', '#FF6B6B', 'string', true, false),
(2, 'max_api_calls', '5000', 'integer', false, false),
(2, 'session_timeout_minutes', '45', 'integer', false, false),
(3, 'theme_color', '#4ECDC4', 'string', true, false),
(3, 'max_api_calls', '15000', 'integer', false, false),
(3, 'session_timeout_minutes', '20', 'integer', false, false),
(3, 'data_retention_days', '365', 'integer', false, false),
(4, 'theme_color', '#95E1D3', 'string', true, false),
(4, 'max_api_calls', '20000', 'integer', false, false),
(4, 'sso_enabled', 'true', 'boolean', false, false),
(4, 'sso_provider', 'okta', 'string', false, false),
(5, 'theme_color', '#3A86FF', 'string', true, false),
(5, 'max_api_calls', '8000', 'integer', false, false),
(5, 'support_chat_enabled', 'true', 'boolean', false, false),
(5, 'timezone', 'America/Los_Angeles', 'string', false, false),
(6, 'theme_color', '#FFBE0B', 'string', true, false),
(6, 'max_api_calls', '12000', 'integer', false, false),
(6, 'email_notifications_enabled', 'false', 'boolean', false, false),
(6, 'timezone', 'America/Los_Angeles', 'string', false, false),
(7, 'theme_color', '#FB5607', 'string', true, false),
(7, 'max_api_calls', '25000', 'integer', false, false),
(7, 'timezone', 'America/Chicago', 'string', false, false),
(7, 'email_notifications_enabled', 'true', 'boolean', false, false),
(8, 'theme_color', '#8338EC', 'string', true, false),
(8, 'max_api_calls', '9000', 'integer', false, false),
(8, 'email_notifications_enabled', 'true', 'boolean', false, false),
(8, 'timezone', 'America/Toronto', 'string', false, false),
(9, 'theme_color', '#2EC4B6', 'string', true, false),
(9, 'max_api_calls', '11000', 'integer', false, false),
(9, 'timezone', 'Europe/London', 'string', false, false),
(9, 'email_notifications_enabled', 'false', 'boolean', false, false),
(10, 'theme_color', '#FF006E', 'string', true, false),
(10, 'max_api_calls', '30000', 'integer', false, false),
(10, 'email_notifications_enabled', 'true', 'boolean', false, false),
(10, 'timezone', 'America/New_York', 'string', false, false),
(11, 'theme_color', '#00B4D8', 'string', true, false),
(11, 'max_api_calls', '7000', 'integer', false, false),
(11, 'timezone', 'America/New_York', 'string', false, false),
(11, 'email_notifications_enabled', 'false', 'boolean', false, false),
(11, 'hipaa_mode', 'true', 'boolean', false, false),
(12, 'theme_color', '#06D6A0', 'string', true, false),
(12, 'max_api_calls', '18000', 'integer', false, false),
(12, 'email_notifications_enabled', 'false', 'boolean', false, false),
(12, 'timezone', 'America/Los_Angeles', 'string', false, false);
