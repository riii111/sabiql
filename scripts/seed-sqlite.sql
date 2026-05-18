PRAGMA foreign_keys = ON;

INSERT INTO users (id, username, email, full_name, phone, avatar_url, bio, location, company, job_title, website, github_url, twitter_handle, timezone, language_preference, password_hash, email_verified, phone_verified, two_factor_enabled, last_login_at, failed_login_attempts, account_status, manager_id) VALUES
(1, 'alice', 'alice@example.com', 'Alice Johnson', '+1-555-0101', 'https://example.com/alice.jpg', 'Software engineer', 'San Francisco, CA', 'Tech Corp', 'Senior Engineer', 'https://alice.dev', 'https://github.com/alice', '@alice_dev', 'America/Los_Angeles', 'en', 'hashed_password_1', 1, 1, 1, datetime('now', '-2 hours'), 0, 'active', NULL),
(2, 'bob', 'bob@example.com', 'Bob Smith', '+1-555-0102', 'https://example.com/bob.jpg', 'Product manager', 'New York, NY', 'Tech Corp', 'Product Manager', 'https://bob.dev', 'https://github.com/bob', '@bob_pm', 'America/New_York', 'en', 'hashed_password_2', 1, 0, 0, datetime('now', '-1 day'), 0, 'active', 1),
(3, 'charlie', 'charlie@example.com', 'Charlie Brown', '+1-555-0103', 'https://example.com/charlie.jpg', 'Design enthusiast', 'London, UK', 'Design Studio', 'UX Designer', 'https://charlie.design', 'https://github.com/charlie', '@charlie_design', 'Europe/London', 'en', 'hashed_password_3', 1, 1, 0, datetime('now', '-3 days'), 0, 'active', NULL),
(4, 'diana', 'diana@example.com', 'Diana Prince', '+1-555-0104', 'https://example.com/diana.jpg', 'Data scientist', 'Austin, TX', 'Data Solutions', 'Data Scientist', 'https://diana.ai', 'https://github.com/diana', '@diana_data', 'America/Chicago', 'en', 'hashed_password_4', 1, 1, 1, datetime('now', '-5 hours'), 0, 'active', NULL),
(5, 'eve', 'eve@example.com', 'Eve Wilson', NULL, NULL, 'Developer advocate', 'Remote', 'Cloud Systems', 'Developer Advocate', 'https://eve.dev', 'https://github.com/eve', '@eve_advocate', 'UTC', 'ja', 'hashed_password_5', 0, 0, 0, NULL, 0, 'active', NULL),
(6, 'frank', 'frank.li@example.com', 'Frank Li', '+1-555-0106', 'https://example.com/frank.jpg', 'Backend engineer', 'Seattle, WA', 'Cloud Systems', 'Backend Engineer', 'https://frank.li', 'https://github.com/frankli', '@frank_backend', 'America/Los_Angeles', 'en', 'hashed_password_6', 1, 1, 0, datetime('now', '-12 hours'), 1, 'active', 5),
(7, 'grace', 'grace.park@example.com', 'Grace Park', '+1-555-0107', 'https://example.com/grace.jpg', 'QA lead', 'Toronto, CA', 'Tech Corp', 'QA Lead', 'https://grace.qa', 'https://github.com/gracepark', '@grace_tests', 'America/Toronto', 'en', 'hashed_password_7', 1, 1, 0, datetime('now', '-4 days'), 0, 'active', 1),
(8, 'henry', 'henry.ng@example.com', 'Henry Ng', NULL, NULL, 'DevOps specialist', 'Vancouver, CA', 'Cloud Systems', 'DevOps Specialist', 'https://henry.ops', 'https://github.com/henryng', '@henry_ops', 'America/Vancouver', 'en', 'hashed_password_8', 1, 0, 0, datetime('now', '-7 days'), 2, 'active', 5),
(9, 'isabel', 'isabel.chen@example.com', 'Isabel Chen', '+1-555-0109', 'https://example.com/isabel.jpg', 'Marketing strategist', 'Sydney, AU', 'Data Solutions', 'Marketing Lead', 'https://isabel.marketing', 'https://github.com/isabelchen', '@isabelgrowth', 'Australia/Sydney', 'en', 'hashed_password_9', 1, 0, 0, datetime('now', '-9 hours'), 0, 'active', 4),
(10, 'jack', 'jack.turner@example.com', 'Jack Turner', NULL, NULL, 'Support engineer', 'Denver, CO', 'Tech Corp', 'Support Engineer', 'https://jack.support', 'https://github.com/jackturner', '@jack_support', 'America/Denver', 'en', 'hashed_password_10', 1, 1, 0, datetime('now', '-2 days'), 0, 'active', 1),
(11, 'kim', 'kim.min@example.com', 'Kim Min', '+1-555-0111', 'https://example.com/kim.jpg', 'Data analyst', 'Seoul, KR', 'Data Solutions', 'Data Analyst', 'https://kim.data', 'https://github.com/kimmin', '@kim_data', 'Asia/Seoul', 'ko', 'hashed_password_11', 1, 1, 0, datetime('now', '-6 days'), 0, 'active', 4),
(12, 'leo', 'leo.martinez@example.com', 'Leo Martinez', NULL, NULL, 'Sales director', 'Miami, FL', 'Cloud Systems', 'Sales Director', 'https://leo.sales', 'https://github.com/leomartinez', '@leo_sales', 'America/New_York', 'en', 'hashed_password_12', 1, 1, 0, datetime('now', '-3 hours'), 0, 'active', NULL),
(13, 'maya', 'maya.patel@example.com', 'Maya Patel', '+1-555-0113', 'https://example.com/maya.jpg', 'Product designer', 'Berlin, DE', 'Design Studio', 'Product Designer', 'https://maya.design', 'https://github.com/mayapatel', '@maya_design', 'Europe/Berlin', 'en', 'hashed_password_13', 1, 1, 1, datetime('now', '-11 days'), 0, 'active', 3),
(14, 'nora', 'nora.hughes@example.com', 'Nora Hughes', NULL, NULL, 'HR manager', 'Chicago, IL', 'Tech Corp', 'HR Manager', 'https://nora.hr', 'https://github.com/norahughes', '@nora_hr', 'America/Chicago', 'en', 'hashed_password_14', 1, 0, 0, datetime('now', '-14 days'), 1, 'active', NULL),
(15, 'owen', 'owen.kim@example.com', 'Owen Kim', '+1-555-0115', 'https://example.com/owen.jpg', 'Security engineer', 'Boston, MA', 'Cloud Systems', 'Security Engineer', 'https://owen.security', 'https://github.com/owenkim', '@owen_sec', 'America/New_York', 'en', 'hashed_password_15', 1, 1, 1, datetime('now', '-1 day'), 0, 'active', 5),
(16, 'peter', 'peter.wang@example.com', 'Peter Wang', '+1-555-0116', 'https://example.com/peter.jpg', 'Frontend developer', 'Portland, OR', 'Tech Corp', 'Frontend Developer', 'https://peter.ui', 'https://github.com/peterwang', '@peter_fe', 'America/Los_Angeles', 'en', 'hashed_password_16', 1, 1, 0, datetime('now', '-8 hours'), 0, 'active', 1),
(17, 'quinn', 'quinn.taylor@example.com', 'Quinn Taylor', NULL, NULL, 'Mobile developer', 'Austin, TX', 'Tech Corp', 'Mobile Developer', 'https://quinn.mobile', 'https://github.com/quinntaylor', '@quinn_mobile', 'America/Chicago', 'en', 'hashed_password_17', 1, 0, 0, datetime('now', '-5 days'), 0, 'active', 1),
(18, 'rachel', 'rachel.green@example.com', 'Rachel Green', '555-0118', 'https://example.com/rachel.jpg', 'Content strategist', 'New York, NY', 'Design Studio', 'Content Lead', 'https://rachel.content', 'https://github.com/rachelgreen', '@rachel_content', 'America/New_York', 'en', 'hashed_password_18', 1, 1, 0, datetime('now', '-2 days'), 0, 'active', 3),
(19, 'sam', 'sam.wilson@example.com', 'Sam Wilson', NULL, NULL, 'Infrastructure engineer', 'Seattle, WA', 'Cloud Systems', 'Infra Engineer', 'https://sam.infra', 'https://github.com/samwilson', '@sam_infra', 'America/Los_Angeles', 'en', 'hashed_password_19', 1, 1, 1, datetime('now', '-6 hours'), 0, 'active', 8),
(20, 'tina', 'tina.rodriguez@example.com', 'Tina Rodriguez', '+1-555-0120', 'https://example.com/tina.jpg', 'Business analyst', 'Miami, FL', 'Data Solutions', 'Business Analyst', 'https://tina.ba', 'https://github.com/tinarodriguez', '@tina_ba', 'America/New_York', 'en', 'hashed_password_20', 1, 1, 0, datetime('now', '-10 days'), 0, 'active', 4);

INSERT INTO sessions (id, user_id, token_hash, ip_address, user_agent, device_type, last_activity_at, expires_at) VALUES
('sess-001', 1, 'hash_session_001', '192.168.1.100', 'Mozilla/5.0 Chrome/120', 'desktop', datetime('now', '-30 minutes'), datetime('now', '+7 days')),
('sess-002', 1, 'hash_session_002', '10.0.0.50', 'Safari/17.0 Mobile', 'mobile', datetime('now', '-2 hours'), datetime('now', '+7 days')),
('sess-003', 2, 'hash_session_003', '192.168.1.101', 'Mozilla/5.0 Firefox/121', 'desktop', datetime('now', '-1 day'), datetime('now', '+6 days')),
('sess-004', 3, 'hash_session_004', '172.16.0.25', 'Mozilla/5.0 Chrome/120', 'desktop', datetime('now', '-3 hours'), datetime('now', '+7 days')),
('sess-005', 4, 'hash_session_005', '192.168.2.50', 'Edge/120', 'desktop', datetime('now', '-5 hours'), datetime('now', '+7 days')),
('sess-006', 5, 'hash_session_006', '10.10.10.100', 'Safari/17.0', 'desktop', datetime('now', '-12 hours'), datetime('now', '+5 days')),
('sess-007', 6, 'hash_session_007', '192.168.1.150', 'Mozilla/5.0 Chrome/120', 'desktop', datetime('now', '-2 days'), datetime('now', '+5 days')),
('sess-008', 7, 'hash_session_008', '172.20.0.75', 'Safari/17.0 Mobile', 'tablet', datetime('now', '-4 hours'), datetime('now', '+7 days'));

INSERT INTO api_keys (id, user_id, name, key_hash, key_prefix, scopes, rate_limit, last_used_at, expires_at, is_active) VALUES
(1, 1, 'Production API', 'hash_key_001', 'sk_prod_', '["read","write","admin"]', 10000, datetime('now', '-1 hour'), datetime('now', '+1 year'), 1),
(2, 1, 'Development API', 'hash_key_002', 'sk_dev_', '["read","write"]', 5000, datetime('now', '-30 minutes'), datetime('now', '+6 months'), 1),
(3, 2, 'CI/CD Integration', 'hash_key_003', 'sk_ci_', '["read","deploy"]', 2000, datetime('now', '-2 hours'), datetime('now', '+3 months'), 1),
(4, 4, 'Analytics Key', 'hash_key_004', 'sk_ana_', '["read"]', 1000, datetime('now', '-1 day'), NULL, 1),
(5, 5, 'Old Key', 'hash_key_005', 'sk_old_', '["read"]', 500, datetime('now', '-90 days'), datetime('now', '-30 days'), 0),
(6, 6, 'Backend Service', 'hash_key_006', 'sk_svc_', '["read","write"]', 15000, datetime('now', '-3 hours'), datetime('now', '+2 years'), 1),
(7, 8, 'DevOps Automation', 'hash_key_007', 'sk_ops_', '["read","write","deploy"]', 8000, datetime('now', '-6 hours'), datetime('now', '+1 year'), 1);

INSERT INTO password_reset_tokens (id, user_id, token_hash, used_at, expires_at) VALUES
(1, 5, 'reset_hash_001', NULL, datetime('now', '+1 hour')),
(2, 8, 'reset_hash_002', datetime('now', '-2 days'), datetime('now', '-1 day')),
(3, 14, 'reset_hash_003', NULL, datetime('now', '+30 minutes'));

INSERT INTO organizations (id, slug, name, description, logo_url, website, email, phone, country, city, business_type, industry, employee_count, founded_year, verified, subscription_tier, subscription_status, owner_id, parent_organization_id) VALUES
(1, 'tech-corp', 'Tech Corp', 'Leading technology company', 'https://example.com/tech-corp.png', 'https://techcorp.com', 'contact@techcorp.com', '+1-555-1000', 'United States', 'San Francisco', 'Corporation', 'Software Development', '500-1000', 2010, 1, 'enterprise', 'active', 1, NULL),
(2, 'design-studio', 'Design Studio', 'Creative design agency', 'https://example.com/design-studio.png', 'https://designstudio.com', 'hello@designstudio.com', '+1-555-1001', 'United Kingdom', 'London', 'Partnership', 'Design & Creative', '50-100', 2015, 1, 'professional', 'active', 3, NULL),
(3, 'data-solutions', 'Data Solutions', 'Advanced analytics platform', 'https://example.com/data-solutions.png', 'https://datasolutions.ai', 'info@datasolutions.ai', '+1-555-1002', 'United States', 'Austin', 'Corporation', 'Data Analytics', '100-250', 2018, 1, 'professional', 'active', 4, NULL),
(4, 'cloud-systems', 'Cloud Systems', 'Cloud infrastructure provider', 'https://example.com/cloud-systems.png', 'https://cloudsystems.io', 'support@cloudsystems.io', '+1-555-1003', 'United States', 'Seattle', 'Corporation', 'Cloud Computing', '250-500', 2012, 1, 'enterprise', 'active', 5, NULL),
(5, 'greenfield-ops', 'Greenfield Ops', 'Managed DevOps and SRE services', 'https://example.com/greenfield-ops.png', 'https://greenfieldops.com', 'hello@greenfieldops.com', '+1-555-1004', 'United States', 'Portland', 'LLC', 'DevOps Services', '25-50', 2019, 1, 'professional', 'active', 6, NULL),
(6, 'northwind-retail', 'Northwind Retail', 'Omnichannel retail platform', 'https://example.com/northwind.png', 'https://northwindretail.com', 'support@northwindretail.com', '+1-555-1005', 'United States', 'Chicago', 'Corporation', 'Retail Tech', '250-500', 2011, 1, 'enterprise', 'active', 7, NULL),
(7, 'brightline-labs', 'Brightline Labs', 'Applied AI research studio', 'https://example.com/brightline.png', 'https://brightlinelabs.ai', 'contact@brightlinelabs.ai', '+1-555-1006', 'Canada', 'Toronto', 'Corporation', 'Artificial Intelligence', '50-100', 2016, 1, 'professional', 'active', 8, NULL),
(8, 'riverbend-media', 'Riverbend Media', 'Streaming media production', 'https://example.com/riverbend.png', 'https://riverbendmedia.com', 'studio@riverbendmedia.com', '+1-555-1007', 'United Kingdom', 'Manchester', 'Corporation', 'Media & Entertainment', '100-250', 2013, 1, 'professional', 'active', 9, NULL),
(9, 'atlas-finance', 'Atlas Finance', 'Risk analytics for fintech', 'https://example.com/atlas-finance.png', 'https://atlasfinance.io', 'hello@atlasfinance.io', '+1-555-1008', 'United States', 'New York', 'Corporation', 'Fintech', '100-250', 2017, 1, 'enterprise', 'active', 10, NULL),
(10, 'openlane-logistics', 'Openlane Logistics', 'Last-mile logistics software', 'https://example.com/openlane.png', 'https://openlane.ai', 'ops@openlane.ai', '+1-555-1009', 'United States', 'Atlanta', 'Corporation', 'Logistics', '50-100', 2020, 1, 'starter', 'active', 11, NULL),
(11, 'zenith-health', 'Zenith Health', 'Healthcare data platform', 'https://example.com/zenith-health.png', 'https://zenithhealth.io', 'info@zenithhealth.io', '+1-555-1010', 'United States', 'San Diego', 'Corporation', 'Health Tech', '250-500', 2014, 1, 'enterprise', 'active', 12, NULL),
(12, 'summit-edtech', 'Summit EdTech', 'Learning analytics and content delivery', 'https://example.com/summit-edtech.png', 'https://summitedtech.com', 'hello@summitedtech.com', '+1-555-1011', 'United States', 'Boston', 'Corporation', 'Education Technology', '50-100', 2019, 1, 'professional', 'active', 15, NULL),
(13, 'tech-corp-asia', 'Tech Corp Asia', 'Asia-Pacific division of Tech Corp', 'https://example.com/tech-corp-asia.png', 'https://asia.techcorp.com', 'asia@techcorp.com', '+81-3-1234-5678', 'Japan', 'Tokyo', 'Corporation', 'Software Development', '100-250', 2018, 1, 'enterprise', 'active', 1, 1),
(14, 'tech-corp-europe', 'Tech Corp Europe', 'European division of Tech Corp', 'https://example.com/tech-corp-eu.png', 'https://eu.techcorp.com', 'europe@techcorp.com', '+44-20-7123-4567', 'United Kingdom', 'London', 'Corporation', 'Software Development', '100-250', 2017, 1, 'enterprise', 'active', 1, 1);

INSERT INTO organization_members (organization_id, user_id, role, permissions, invited_by, joined_at) VALUES
(1, 1, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-3 years')),
(1, 2, 'admin', '["admin","member_management"]', 1, datetime('now', '-2 years')),
(1, 7, 'member', '["read","write"]', 1, datetime('now', '-1 year')),
(1, 10, 'member', '["read","write","support"]', 2, datetime('now', '-6 months')),
(1, 16, 'member', '["read","write"]', 2, datetime('now', '-8 months')),
(1, 17, 'member', '["read","write"]', 2, datetime('now', '-4 months')),
(2, 3, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-2 years')),
(2, 13, 'admin', '["admin","member_management"]', 3, datetime('now', '-1 year')),
(2, 18, 'member', '["read","write"]', 3, datetime('now', '-6 months')),
(3, 4, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-2 years')),
(3, 9, 'admin', '["admin","analytics"]', 4, datetime('now', '-1 year')),
(3, 11, 'member', '["read","write","analytics"]', 4, datetime('now', '-8 months')),
(3, 20, 'member', '["read","analytics"]', 9, datetime('now', '-3 months')),
(4, 5, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-2 years')),
(4, 6, 'admin', '["admin","deploy"]', 5, datetime('now', '-1 year')),
(4, 8, 'member', '["read","write","deploy"]', 5, datetime('now', '-10 months')),
(4, 15, 'member', '["read","security"]', 5, datetime('now', '-7 months')),
(4, 19, 'member', '["read","write","infra"]', 8, datetime('now', '-5 months')),
(5, 6, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-1 year')),
(6, 7, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-4 years')),
(7, 8, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-2 years')),
(8, 9, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-3 years')),
(9, 10, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-2 years')),
(10, 11, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-1 year')),
(11, 12, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-3 years')),
(12, 15, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-1 year')),
(13, 1, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-2 years')),
(14, 1, 'owner', '["admin","billing","member_management"]', NULL, datetime('now', '-2 years'));

INSERT INTO departments (id, organization_id, name, code, description, parent_department_id, manager_id, budget, headcount, is_active) VALUES
(1, 1, 'Engineering', 'ENG', 'Software engineering department', NULL, 1, 5000000.00, 150, 1),
(2, 1, 'Backend Team', 'ENG-BE', 'Backend development', 1, 6, 1500000.00, 40, 1),
(3, 1, 'Frontend Team', 'ENG-FE', 'Frontend development', 1, 16, 1200000.00, 35, 1),
(4, 1, 'Mobile Team', 'ENG-MOB', 'Mobile app development', 1, 17, 1000000.00, 25, 1),
(5, 1, 'QA Team', 'ENG-QA', 'Quality assurance', 1, 7, 800000.00, 20, 1),
(6, 1, 'Product', 'PROD', 'Product management', NULL, 2, 1500000.00, 25, 1),
(7, 1, 'Support', 'SUP', 'Customer support', NULL, 10, 800000.00, 30, 1),
(8, 1, 'HR', 'HR', 'Human resources', NULL, 14, 500000.00, 10, 1),
(9, 2, 'Design', 'DES', 'Design department', NULL, 3, 800000.00, 20, 1),
(10, 2, 'UX Research', 'DES-UX', 'User experience research', 9, 13, 300000.00, 8, 1),
(11, 2, 'Visual Design', 'DES-VIS', 'Visual and brand design', 9, 18, 300000.00, 8, 1),
(12, 3, 'Analytics', 'ANA', 'Data analytics', NULL, 4, 1200000.00, 30, 1),
(13, 3, 'Data Engineering', 'ANA-DE', 'Data engineering team', 12, 11, 600000.00, 15, 1),
(14, 4, 'Infrastructure', 'INF', 'Cloud infrastructure', NULL, 5, 2000000.00, 40, 1),
(15, 4, 'DevOps', 'INF-OPS', 'DevOps team', 14, 8, 800000.00, 15, 1),
(16, 4, 'Security', 'INF-SEC', 'Security team', 14, 15, 600000.00, 10, 1);

INSERT INTO categories (id, slug, name, description, icon, parent_id, sort_order, is_active) VALUES
(1, 'software', 'Software', 'Software products and services', 'code', NULL, 1, 1),
(2, 'cloud-services', 'Cloud Services', 'Cloud-based solutions', 'cloud', 1, 1, 1),
(3, 'analytics', 'Analytics', 'Analytics and reporting tools', 'chart-bar', 1, 2, 1),
(4, 'developer-tools', 'Developer Tools', 'Tools for software developers', 'wrench', 1, 3, 1),
(5, 'security', 'Security', 'Security and compliance tools', 'shield', 1, 4, 1),
(6, 'design-tools', 'Design Tools', 'Design and creative software', 'palette', 1, 5, 1),
(7, 'infrastructure', 'Infrastructure', 'Infrastructure management', 'server', 2, 1, 1),
(8, 'storage', 'Storage', 'Cloud storage solutions', 'database', 2, 2, 1),
(9, 'compute', 'Compute', 'Cloud compute resources', 'cpu', 2, 3, 1),
(10, 'business-intelligence', 'Business Intelligence', 'BI and dashboards', 'presentation-chart-bar', 3, 1, 1),
(11, 'data-science', 'Data Science', 'ML and data science tools', 'beaker', 3, 2, 1),
(12, 'marketing-analytics', 'Marketing Analytics', 'Marketing performance', 'megaphone', 3, 3, 1),
(13, 'ides', 'IDEs', 'Integrated development environments', 'code-bracket', 4, 1, 1),
(14, 'apis', 'APIs', 'API management and gateways', 'arrows-right-left', 4, 2, 1),
(15, 'testing', 'Testing', 'Testing and QA tools', 'check-circle', 4, 3, 1),
(16, 'identity', 'Identity', 'Identity and access management', 'user-circle', 5, 1, 1),
(17, 'threat-detection', 'Threat Detection', 'Security monitoring', 'eye', 5, 2, 1),
(18, 'compliance', 'Compliance', 'Compliance and audit', 'clipboard-check', 5, 3, 1),
(19, 'prototyping', 'Prototyping', 'Design prototyping tools', 'cursor-arrow-rays', 6, 1, 1),
(20, 'collaboration', 'Collaboration', 'Design collaboration', 'users', 6, 2, 1),
(21, 'hardware', 'Hardware', 'Physical products', 'computer-desktop', NULL, 2, 1),
(22, 'networking', 'Networking', 'Network equipment', 'globe', 21, 1, 1),
(23, 'peripherals', 'Peripherals', 'Computer peripherals', 'square-3-stack-3d', 21, 2, 1);

INSERT INTO tags (id, slug, name, color) VALUES
(1, 'enterprise', 'Enterprise', '#2563EB'),
(2, 'startup-friendly', 'Startup Friendly', '#10B981'),
(3, 'open-source', 'Open Source', '#8B5CF6'),
(4, 'saas', 'SaaS', '#F59E0B'),
(5, 'on-premise', 'On-Premise', '#6B7280'),
(6, 'api-first', 'API First', '#EC4899'),
(7, 'no-code', 'No-Code', '#14B8A6'),
(8, 'ai-powered', 'AI Powered', '#EF4444'),
(9, 'real-time', 'Real-time', '#3B82F6'),
(10, 'scalable', 'Scalable', '#22C55E'),
(11, 'secure', 'Secure', '#F97316'),
(12, 'mobile-ready', 'Mobile Ready', '#A855F7'),
(13, 'integrations', 'Integrations', '#64748B'),
(14, 'free-tier', 'Free Tier', '#06B6D4'),
(15, 'premium', 'Premium', '#D97706');

WITH RECURSIVE seq(n) AS (
    VALUES(1)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 120
)
INSERT INTO products (id, sku, name, description, category_id, price, cost_price, discount_price, tax_rate, stock_quantity, weight, supplier_id, organization_id, is_active, is_featured, rating, review_count, metadata)
SELECT
    n,
    printf('PROD-%03d', n),
    CASE n % 12
        WHEN 0 THEN 'Cloud Storage Pro'
        WHEN 1 THEN 'Analytics Dashboard'
        WHEN 2 THEN 'Design Pro Suite'
        WHEN 3 THEN 'API Gateway'
        WHEN 4 THEN 'Security Suite'
        WHEN 5 THEN 'Mobile Dev Kit'
        WHEN 6 THEN 'Database Manager'
        WHEN 7 THEN 'Content Hub'
        WHEN 8 THEN 'Compliance Monitor'
        WHEN 9 THEN 'Retail Insights'
        WHEN 10 THEN 'Media Pipeline'
        ELSE 'AI Notebook'
    END || ' ' || n,
    'Seed product for SQLite browsing and editing checks',
    1 + (n % 23),
    99.00 + (n * 17.35),
    40.00 + (n * 6.25),
    CASE WHEN n % 4 = 0 THEN 79.00 + (n * 15.10) ELSE NULL END,
    10.0,
    30 + (n % 90),
    CASE WHEN n % 3 = 0 THEN 1.25 + (n / 10.0) ELSE NULL END,
    1 + (n % 12),
    1 + (n % 14),
    CASE WHEN n % 17 = 0 THEN 0 ELSE 1 END,
    CASE WHEN n % 5 = 0 THEN 1 ELSE 0 END,
    3.5 + ((n % 15) / 10.0),
    n * 3,
    '{"source":"sqlite-seed","tier":"dev"}'
FROM seq;

INSERT INTO product_tags (product_id, tag_id)
SELECT p.id, 1 + (p.id % 15) FROM products p
UNION
SELECT p.id, 1 + ((p.id + 4) % 15) FROM products p
UNION
SELECT p.id, 1 + ((p.id + 8) % 15) FROM products p WHERE p.id % 2 = 0;

INSERT INTO user_favorites (user_id, product_id, notes)
SELECT 1 + (id % 20), id, CASE WHEN id % 3 = 0 THEN 'Evaluate for the next project' ELSE NULL END
FROM products
WHERE id <= 80 AND id % 2 = 0;

INSERT INTO warehouses (id, organization_id, code, name, address, city, country, latitude, longitude, capacity, manager_id, is_active) VALUES
(1, 1, 'WH-SF-01', 'San Francisco Main', '100 Tech Street', 'San Francisco', 'United States', 37.7749, -122.4194, 10000, 1, 1),
(2, 1, 'WH-NY-01', 'New York Hub', '200 Commerce Ave', 'New York', 'United States', 40.7128, -74.0060, 8000, 2, 1),
(3, 1, 'WH-SEA-01', 'Seattle Fulfillment', '300 Cloud Drive', 'Seattle', 'United States', 47.6062, -122.3321, 12000, NULL, 1),
(4, 6, 'WH-CHI-01', 'Chicago Distribution', '500 Retail Lane', 'Chicago', 'United States', 41.8781, -87.6298, 15000, 7, 1),
(5, 6, 'WH-ATL-01', 'Atlanta Logistics', '600 Supply Chain Rd', 'Atlanta', 'United States', 33.7490, -84.3880, 20000, NULL, 1),
(6, 10, 'WH-ATL-02', 'Atlanta Express', '700 Fast Delivery Way', 'Atlanta', 'United States', 33.7550, -84.3900, 5000, 11, 1),
(7, 11, 'WH-SD-01', 'San Diego Medical', '800 Health Blvd', 'San Diego', 'United States', 32.7157, -117.1611, 6000, 12, 1);

INSERT INTO stock_levels (warehouse_id, product_id, quantity, reserved_quantity, reorder_point, last_counted_at)
SELECT w.id, p.id, 20 + ((w.id * p.id) % 80), (w.id * p.id) % 12, 10 + (p.id % 15), datetime('now', '-' || (p.id % 10) || ' days')
FROM warehouses w
JOIN products p ON p.id <= 70
WHERE (w.id + p.id) % 3 = 0;

INSERT INTO inventory_movements (warehouse_id, product_id, movement_type, quantity, reference_type, reference_id, notes, created_by, created_at)
SELECT
    1 + (n % 7),
    1 + (n % 120),
    CASE n % 4 WHEN 0 THEN 'purchase' WHEN 1 THEN 'sale' WHEN 2 THEN 'adjustment' ELSE 'return' END,
    1 + (n % 30),
    CASE n % 3 WHEN 0 THEN 'order' WHEN 1 THEN 'manual' ELSE 'cycle_count' END,
    n,
    'Generated movement',
    1 + (n % 20),
    datetime('now', '-' || n || ' hours')
FROM (WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 240) SELECT n FROM seq);

INSERT INTO shipping_zones (id, organization_id, name, countries, is_active) VALUES
(1, 1, 'North America', '["US","CA","MX"]', 1),
(2, 1, 'Europe', '["GB","DE","FR","NL"]', 1),
(3, 6, 'Retail Domestic', '["US"]', 1),
(4, 10, 'Logistics Express', '["US","CA"]', 1),
(5, 11, 'Healthcare Domestic', '["US"]', 1);

INSERT INTO shipping_rates (id, zone_id, name, min_weight, max_weight, min_order_amount, max_order_amount, rate, estimated_days_min, estimated_days_max, is_active) VALUES
(1, 1, 'Standard', 0, 20, 0, 1000, 9.99, 3, 5, 1),
(2, 1, 'Express', 0, 20, 0, 1000, 19.99, 1, 2, 1),
(3, 2, 'International Standard', 0, 10, 0, 1500, 29.99, 5, 9, 1),
(4, 3, 'Retail Ground', 0, 50, 0, 2500, 14.99, 2, 4, 1),
(5, 4, 'Same Day', 0, 5, 0, 500, 39.99, 0, 1, 1),
(6, 5, 'Medical Priority', 0, 15, 0, 5000, 49.99, 1, 1, 1);

INSERT INTO coupons (id, code, organization_id, description, discount_type, discount_value, min_order_amount, max_discount_amount, usage_limit, usage_count, starts_at, expires_at, is_active) VALUES
(1, 'WELCOME10', 1, 'Welcome discount', 'percentage', 10, 100, 100, 1000, 125, datetime('now', '-30 days'), datetime('now', '+60 days'), 1),
(2, 'ENTERPRISE25', 1, 'Enterprise plan promotion', 'percentage', 25, 1000, 500, 100, 12, datetime('now', '-10 days'), datetime('now', '+20 days'), 1),
(3, 'DESIGN50', 2, 'Design studio promotion', 'fixed', 50, 200, 50, 250, 44, datetime('now', '-5 days'), datetime('now', '+90 days'), 1),
(4, 'ANALYTICS15', 3, 'Analytics launch', 'percentage', 15, 500, 200, 300, 73, datetime('now', '-15 days'), datetime('now', '+45 days'), 1),
(5, 'EXPIRED20', 4, 'Expired campaign', 'percentage', 20, 100, 100, 100, 99, datetime('now', '-90 days'), datetime('now', '-1 day'), 0);

WITH RECURSIVE seq(n) AS (
    VALUES(1)
    UNION ALL
    SELECT n + 1 FROM seq WHERE n < 180
)
INSERT INTO orders (id, order_number, user_id, organization_id, status, payment_status, fulfillment_status, total_amount, subtotal, tax_amount, shipping_amount, discount_amount, coupon_id, customer_email, shipping_city, shipping_country, tracking_number, carrier, warehouse_id, shipped_at, delivered_at, created_at)
SELECT
    n,
    printf('ORD-2026-%04d', n),
    1 + (n % 20),
    1 + (n % 14),
    CASE n % 6 WHEN 0 THEN 'pending' WHEN 1 THEN 'processing' WHEN 2 THEN 'completed' WHEN 3 THEN 'completed' WHEN 4 THEN 'cancelled' ELSE 'completed' END,
    CASE n % 5 WHEN 0 THEN 'pending' WHEN 1 THEN 'paid' WHEN 2 THEN 'paid' WHEN 3 THEN 'refunded' ELSE 'paid' END,
    CASE n % 5 WHEN 0 THEN 'unshipped' WHEN 1 THEN 'in_transit' WHEN 2 THEN 'delivered' WHEN 3 THEN 'returned' ELSE 'delivered' END,
    120.00 + (n * 23.75),
    100.00 + (n * 21.25),
    10.00 + (n % 20),
    CASE WHEN n % 4 = 0 THEN 0 ELSE 9.99 END,
    CASE WHEN n % 5 = 0 THEN 25.00 ELSE 0 END,
    CASE WHEN n % 7 = 0 THEN 1 + (n % 5) ELSE NULL END,
    (SELECT email FROM users WHERE id = 1 + (n % 20)),
    CASE n % 8 WHEN 0 THEN 'San Francisco' WHEN 1 THEN 'New York' WHEN 2 THEN 'London' WHEN 3 THEN 'Austin' WHEN 4 THEN 'Seattle' WHEN 5 THEN 'Chicago' WHEN 6 THEN 'Atlanta' ELSE 'San Diego' END,
    CASE n % 4 WHEN 0 THEN 'United States' WHEN 1 THEN 'United Kingdom' WHEN 2 THEN 'Canada' ELSE 'Japan' END,
    CASE WHEN n % 4 = 0 THEN NULL ELSE printf('TRK-%05d', n) END,
    CASE n % 4 WHEN 0 THEN NULL WHEN 1 THEN 'UPS' WHEN 2 THEN 'FedEx' ELSE 'DHL' END,
    CASE WHEN n % 6 = 0 THEN NULL ELSE 1 + (n % 7) END,
    CASE WHEN n % 4 = 0 THEN NULL ELSE datetime('now', '-' || (n % 20) || ' days') END,
    CASE WHEN n % 3 = 0 THEN datetime('now', '-' || (n % 15) || ' days') ELSE NULL END,
    datetime('now', '-' || n || ' hours')
FROM seq;

INSERT INTO order_items (order_id, product_id, quantity, unit_price, discount_price, line_total)
SELECT
    o.id,
    1 + (o.id % 120),
    1 + (o.id % 4),
    99.00 + ((o.id % 90) * 7.50),
    CASE WHEN o.id % 5 = 0 THEN 89.00 + ((o.id % 90) * 7.00) ELSE NULL END,
    (1 + (o.id % 4)) * (99.00 + ((o.id % 90) * 7.50))
FROM orders o
UNION ALL
SELECT
    o.id,
    1 + ((o.id + 17) % 120),
    1 + ((o.id + 1) % 3),
    129.00 + ((o.id % 80) * 8.25),
    NULL,
    (1 + ((o.id + 1) % 3)) * (129.00 + ((o.id % 80) * 8.25))
FROM orders o
WHERE o.id % 2 = 0;

INSERT INTO payments (order_id, amount, currency, payment_method, payment_status, transaction_id, risk_score, created_at)
SELECT
    id,
    total_amount,
    'USD',
    CASE id % 4 WHEN 0 THEN 'card' WHEN 1 THEN 'bank_transfer' WHEN 2 THEN 'paypal' ELSE 'invoice' END,
    payment_status,
    CASE WHEN payment_status = 'pending' THEN NULL ELSE printf('txn_%06d', id) END,
    (id % 100) / 10.0,
    datetime(created_at, '+10 minutes')
FROM orders
WHERE payment_status <> 'pending';

INSERT INTO coupon_usages (coupon_id, order_id, user_id, discount_applied)
SELECT coupon_id, id, user_id, discount_amount
FROM orders
WHERE coupon_id IS NOT NULL;

INSERT INTO campaigns (id, organization_id, name, campaign_type, status, budget, spent, starts_at, ends_at) VALUES
(1, 1, 'Spring Platform Push', 'email', 'active', 50000, 18750, datetime('now', '-20 days'), datetime('now', '+30 days')),
(2, 2, 'Design Tool Launch', 'social', 'active', 20000, 6200, datetime('now', '-10 days'), datetime('now', '+40 days')),
(3, 3, 'Analytics Webinar', 'webinar', 'paused', 15000, 9100, datetime('now', '-15 days'), datetime('now', '+10 days')),
(4, 4, 'Cloud Security Month', 'content', 'active', 30000, 12800, datetime('now', '-25 days'), datetime('now', '+35 days')),
(5, 10, 'Logistics Route Promo', 'email', 'draft', 12000, 0, datetime('now', '+5 days'), datetime('now', '+50 days')),
(6, 11, 'Healthcare Compliance', 'event', 'completed', 40000, 38900, datetime('now', '-80 days'), datetime('now', '-10 days'));

INSERT INTO promotions (campaign_id, organization_id, product_id, promotion_type, discount_value, starts_at, ends_at, is_active)
SELECT 1 + (id % 6), organization_id, id, CASE id % 3 WHEN 0 THEN 'percentage' WHEN 1 THEN 'fixed' ELSE 'bundle' END, 5 + (id % 25), datetime('now', '-7 days'), datetime('now', '+21 days'), CASE WHEN id % 11 = 0 THEN 0 ELSE 1 END
FROM products
WHERE id <= 60;

INSERT INTO reviews (id, product_id, user_id, order_id, rating, title, content, pros, cons, is_verified_purchase, helpful_count, report_count, moderation_status, moderated_by, moderated_at, created_at)
SELECT
    n,
    1 + (n % 120),
    1 + (n % 20),
    CASE WHEN n <= 180 THEN n ELSE NULL END,
    1 + (n % 5),
    CASE n % 4 WHEN 0 THEN 'Excellent fit' WHEN 1 THEN 'Solid option' WHEN 2 THEN 'Good value' ELSE 'Needs polish' END,
    'Generated review for browsing long result sets',
    'Easy setup, useful reporting',
    CASE WHEN n % 3 = 0 THEN 'Advanced configuration takes time' ELSE NULL END,
    CASE WHEN n % 2 = 0 THEN 1 ELSE 0 END,
    n % 90,
    n % 4,
    CASE WHEN n % 7 = 0 THEN 'pending' ELSE 'approved' END,
    CASE WHEN n % 7 = 0 THEN NULL ELSE 2 END,
    CASE WHEN n % 7 = 0 THEN NULL ELSE datetime('now', '-' || (n % 30) || ' days') END,
    datetime('now', '-' || n || ' hours')
FROM (WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 220) SELECT n FROM seq);

INSERT INTO review_votes (review_id, user_id, is_helpful)
SELECT r.id, 1 + ((r.id + u.id) % 20), CASE WHEN (r.id + u.id) % 3 = 0 THEN 0 ELSE 1 END
FROM reviews r
JOIN users u ON u.id <= 3
WHERE r.id <= 120;

INSERT INTO notifications (user_id, notification_type, title, message, data, is_read, read_at, action_url, created_at)
SELECT
    1 + (n % 20),
    CASE n % 6 WHEN 0 THEN 'order_shipped' WHEN 1 THEN 'review_approved' WHEN 2 THEN 'security' WHEN 3 THEN 'campaign_live' WHEN 4 THEN 'refund_processed' ELSE 'system' END,
    'Notification ' || n,
    'Generated notification for SQLite seed data',
    '{"source":"sqlite-seed"}',
    CASE WHEN n % 4 = 0 THEN 0 ELSE 1 END,
    CASE WHEN n % 4 = 0 THEN NULL ELSE datetime('now', '-' || n || ' minutes') END,
    '/notifications/' || n,
    datetime('now', '-' || n || ' minutes')
FROM (WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 160) SELECT n FROM seq);

INSERT INTO notification_preferences (user_id, notification_type, email_enabled, push_enabled, sms_enabled, in_app_enabled)
SELECT u.id, t.notification_type, 1, CASE WHEN u.id % 3 = 0 THEN 0 ELSE 1 END, CASE WHEN u.id % 10 = 0 THEN 1 ELSE 0 END, 1
FROM users u
JOIN (
    SELECT 'order_shipped' AS notification_type
    UNION ALL SELECT 'review_approved'
    UNION ALL SELECT 'security'
    UNION ALL SELECT 'campaign_live'
    UNION ALL SELECT 'refund_processed'
) t;

INSERT INTO analytics_events (event_type, user_id, session_id, organization_id, properties, page_url, referrer_url, ip_address, country, city, device_type, browser, os, created_at)
SELECT
    CASE n % 12
        WHEN 0 THEN 'page_view'
        WHEN 1 THEN 'product_view'
        WHEN 2 THEN 'add_to_cart'
        WHEN 3 THEN 'search'
        WHEN 4 THEN 'login'
        WHEN 5 THEN 'logout'
        WHEN 6 THEN 'signup_start'
        WHEN 7 THEN 'purchase'
        WHEN 8 THEN 'api_call'
        WHEN 9 THEN 'error'
        WHEN 10 THEN 'feature_used'
        ELSE 'feedback'
    END,
    CASE WHEN n % 9 = 0 THEN NULL ELSE 1 + (n % 20) END,
    printf('event-session-%03d', n % 80),
    1 + (n % 14),
    '{"source":"sqlite-seed","sample":true}',
    '/products/' || (1 + (n % 120)),
    CASE WHEN n % 4 = 0 THEN NULL ELSE '/home' END,
    '10.0.' || (n % 256) || '.' || ((n * 7) % 256),
    CASE n % 8 WHEN 0 THEN 'US' WHEN 1 THEN 'GB' WHEN 2 THEN 'JP' WHEN 3 THEN 'DE' WHEN 4 THEN 'CA' WHEN 5 THEN 'AU' WHEN 6 THEN 'KR' ELSE 'FR' END,
    CASE n % 10 WHEN 0 THEN 'San Francisco' WHEN 1 THEN 'New York' WHEN 2 THEN 'London' WHEN 3 THEN 'Tokyo' WHEN 4 THEN 'Berlin' WHEN 5 THEN 'Toronto' WHEN 6 THEN 'Sydney' WHEN 7 THEN 'Seoul' WHEN 8 THEN 'Paris' ELSE 'Austin' END,
    CASE n % 3 WHEN 0 THEN 'desktop' WHEN 1 THEN 'mobile' ELSE 'tablet' END,
    CASE n % 4 WHEN 0 THEN 'Chrome' WHEN 1 THEN 'Firefox' WHEN 2 THEN 'Safari' ELSE 'Edge' END,
    CASE n % 5 WHEN 0 THEN 'macOS' WHEN 1 THEN 'Windows' WHEN 2 THEN 'Linux' WHEN 3 THEN 'Android' ELSE 'iOS' END,
    datetime('now', '-' || n || ' minutes')
FROM (WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 2000) SELECT n FROM seq);

INSERT INTO daily_metrics (organization_id, metric_date, metric_name, metric_value, dimensions)
SELECT
    1 + (org.n % 14),
    date('now', '-' || day.n || ' days'),
    metric.name,
    (org.n * 1000) + (day.n * 37) + metric.offset,
    metric.dimensions
FROM (WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 14) SELECT n FROM seq) org
JOIN (WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 30) SELECT n FROM seq) day
JOIN (
    SELECT 'daily_revenue' AS name, 0 AS offset, '{"currency":"USD"}' AS dimensions
    UNION ALL SELECT 'daily_orders', 100, NULL
    UNION ALL SELECT 'active_users', 200, NULL
    UNION ALL SELECT 'api_calls', 300, '{"tier":"enterprise"}'
) metric;

INSERT INTO audit_logs (user_id, organization_id, table_name, record_id, action, old_values, new_values, changed_fields, ip_address, user_agent, request_id, created_at)
SELECT
    1 + (n % 20),
    1 + (n % 14),
    CASE n % 7 WHEN 0 THEN 'users' WHEN 1 THEN 'products' WHEN 2 THEN 'organizations' WHEN 3 THEN 'orders' WHEN 4 THEN 'payments' WHEN 5 THEN 'coupons' ELSE 'analytics_events' END,
    CAST(1 + (n % 200) AS TEXT),
    CASE n % 4 WHEN 0 THEN 'INSERT' WHEN 1 THEN 'UPDATE' WHEN 2 THEN 'UPDATE' ELSE 'DELETE' END,
    CASE WHEN n % 4 = 0 THEN NULL ELSE '{"status":"old"}' END,
    CASE WHEN n % 4 = 3 THEN NULL ELSE '{"status":"new"}' END,
    '["status"]',
    '192.168.' || (n % 256) || '.' || ((n * 3) % 256),
    'SQLite seed agent',
    printf('req-%06d', n),
    datetime('now', '-' || n || ' minutes')
FROM (WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n + 1 FROM seq WHERE n < 1000) SELECT n FROM seq);

INSERT INTO agent_threads (id, external_thread_id, title, agent_name, model, goal, status, metadata, created_at) VALUES
(1, 'thread-ai-001', 'Investigate flaky SQLite inspector rendering', 'codex-local', 'gpt-5.2', 'Review a local SQLite database produced by an AI agent and identify result-pane UX issues for long text cells.', 'active', '{"workspace":"sabiql","source":"sqlite-seed"}', datetime('now', '-3 days')),
(2, 'thread-ai-002', 'Summarize customer research notes', 'research-agent', 'gpt-5.2-mini', 'Turn long interview transcripts into compact product insights while preserving citations.', 'archived', '{"workspace":"research","source":"sqlite-seed"}', datetime('now', '-8 days')),
(3, 'thread-ai-003', 'Generate migration plan for local app state', 'planner-agent', 'gpt-5.2', 'Draft a stepwise migration plan from JSON files to SQLite-backed local memory.', 'active', '{"workspace":"desktop-agent","source":"sqlite-seed"}', datetime('now', '-1 day'));

INSERT INTO agent_messages (id, thread_id, role, turn_index, content, token_count, metadata, created_at) VALUES
(1, 1, 'system', 0, 'You are a local coding agent. Keep answers compact, inspect real files before proposing changes, and preserve user edits. When browsing SQLite-backed memory stores, prefer read-only investigation unless explicitly asked to mutate data.', 42, '{"kind":"instruction"}', datetime('now', '-3 days')),
(2, 1, 'user', 1, 'I opened a SQLite database used by an AI desktop app. Some cells contain very long prompts, Markdown summaries, and JSON tool outputs. The grid view truncates everything, so I cannot tell whether the data is valid without copying values out to another tool. Please analyze what a better TUI experience should look like.', 58, '{"kind":"prompt","contains_long_text":true}', datetime('now', '-3 days', '+2 minutes')),
(3, 1, 'assistant', 2, 'The result pane should keep the grid dense and scannable, then provide a focused detail view for the selected cell. A long prompt or memory body should not expand row height inside the table because that makes navigation unstable. A modal can preserve line breaks, wrap paragraphs, support vertical scrolling, and optionally switch into a structured JSON view when the content parses as JSON.\n\nThe interaction should be read-first. Inline edit is a separate action with stronger guardrails, because opening a long prompt for inspection should never feel like the user is about to modify application memory. This is especially important for SQLite files because they are often live local state for an agent, not disposable reporting replicas.\n\nA useful first version is a cell detail modal with four rendering modes: plain text, JSON, empty/NULL, and binary or invalid UTF-8. Plain text keeps Markdown readable without trying to become a full Markdown renderer. JSON uses the existing JSONB modal behavior where possible. Binary values should show size and a short hex preview instead of corrupt text.', 178, '{"kind":"analysis","contains_newlines":true}', datetime('now', '-3 days', '+5 minutes')),
(4, 2, 'user', 1, 'Here are raw interview notes. Please preserve the important wording.\n\nParticipant A said the tool feels fast until they hit a cell that contains a whole generated answer. At that point the table stops being useful because the preview is only a narrow slice of the value.\n\nParticipant B said they expect Enter to mean open or inspect, not edit. They were worried about accidentally changing memory records while exploring an agent database.\n\nParticipant C said JSON payloads should be formatted automatically, because a single-line object with escaped newlines is unreadable in a terminal grid.', 111, '{"kind":"research_notes","contains_newlines":true}', datetime('now', '-8 days')),
(5, 2, 'assistant', 2, 'Three product implications stand out:\n\n1. Keep table navigation stable. Do not let long values resize rows.\n2. Add an inspect action for the selected cell. Treat it as read-only by default.\n3. Reuse existing structured JSON affordances instead of inventing a separate viewer.\n\nThe most concrete UI gap is not SQLite-specific. SQLite makes it visible because local AI tools commonly store prompts, transcripts, tool outputs, and memory summaries in TEXT columns, but PostgreSQL users with JSONB and TEXT columns will benefit from the same design.', 103, '{"kind":"summary","contains_markdown":true}', datetime('now', '-8 days', '+7 minutes')),
(6, 3, 'user', 1, 'Draft a migration plan. The current app writes one JSON file per conversation. Each file contains messages, tool call results, compact summaries, and long-term memories. We want a single SQLite database so the desktop app can query recent work quickly, but we need to preserve debuggability.', 55, '{"kind":"planning_prompt"}', datetime('now', '-1 day')),
(7, 3, 'assistant', 2, 'Recommended migration plan:\n\nPhase 1 keeps the JSON files as source of truth and builds a SQLite mirror. The mirror should include conversations, messages, tool calls, and memory items. Every imported row stores the original file path and byte offset where possible, so debugging can still jump back to the old artifact.\n\nPhase 2 changes reads to prefer SQLite while writes still update JSON first. A background verifier compares row counts, message counts, and content hashes. Any mismatch disables SQLite reads for that workspace.\n\nPhase 3 moves writes to SQLite and exports periodic JSON snapshots for recovery. At that point the UI should include a long-cell inspector because prompts, responses, and memory bodies are no longer convenient to inspect as files.', 145, '{"kind":"plan","contains_newlines":true}', datetime('now', '-1 day', '+9 minutes'));

INSERT INTO agent_tool_calls (message_id, tool_name, arguments_json, result_text, status, elapsed_ms, created_at) VALUES
(3, 'sqlite_schema_scan', '{"database":"/tmp/agent-memory.sqlite3","tables":["agent_messages","agent_memory_items"],"read_only":true}', 'Found 24 user tables. Long text candidates: agent_messages.content, agent_memory_items.body, agent_tool_calls.result_text. JSON candidates: metadata, arguments_json, tags.', 'ok', 184, datetime('now', '-3 days', '+4 minutes')),
(5, 'summarize_transcript', '{"mode":"extractive","preserve_quotes":true,"max_points":5}', 'The clearest repeated observation is that long cells need a separate reading surface. Participants did not ask for a spreadsheet-like expanded row; they asked for a focused view that keeps the grid usable.', 'ok', 922, datetime('now', '-8 days', '+6 minutes')),
(7, 'migration_risk_check', '{"source":"conversation-json","target":"sqlite","checks":["hash","row-count","foreign-key"]}', 'Risk notes:\n- Message ordering must remain stable across imports.\n- Tool result payloads may contain escaped JSON inside text.\n- Memory summaries are short, but memory bodies can exceed several thousand characters.', 'ok', 441, datetime('now', '-1 day', '+8 minutes'));

INSERT INTO agent_memory_items (id, thread_id, memory_key, memory_type, summary, body, embedding_model, importance, tags, created_at) VALUES
(1, 1, 'ux.long-cell-detail-modal', 'product_insight', 'Long AI memory cells need a read-focused detail modal.', 'SQLite-backed AI memory databases often store prompts, generated answers, tool outputs, and compressed memories as plain TEXT. A terminal table is good for scanning rows, but it is a poor reading surface for a single cell containing paragraphs of Markdown or JSON. The proposed behavior is to keep the result grid compact and add a selected-cell detail modal.\n\nThe modal should preserve line breaks, wrap text, and scroll vertically. It should not imply edit mode. For JSON-looking values, it should reuse the existing JSON/JSONB viewing behavior so users get familiar formatting rather than a second, inconsistent viewer.\n\nThis memory is intentionally verbose so local database clients can test long-cell rendering without relying on production data.', 'text-embedding-3-large', 0.92, '["sqlite","ai-memory","ux","long-text"]', datetime('now', '-3 days')),
(2, 2, 'research.enter-should-inspect', 'user_research', 'Users expect inspect and edit to be separate actions.', 'Participants exploring agent memory stores described a strong difference between reading a value and changing it. The table interaction should respect that distinction. Opening a long value should be reversible, read-only, and low risk. Editing should remain guarded, explicit, and visually different.\n\nThis matters for SQLite because many local apps treat the database file as live state. A user may open a database to debug an agent run, not to perform data maintenance. Accidentally writing to that file can change the app behavior they are trying to understand.', 'text-embedding-3-small', 0.84, '["research","read-only","sqlite"]', datetime('now', '-8 days')),
(3, 3, 'migration.sqlite-debuggability', 'architecture_note', 'SQLite migration should keep memory rows debuggable.', 'When migrating agent memory from JSON files to SQLite, preserve enough provenance to debug imported rows. Suggested fields include source file path, source record id, content hash, import timestamp, and schema version. Long text fields should remain inspectable in the TUI because the database becomes the primary operational artifact after migration.\n\nA useful database client should make the common debugging path fast: find a thread, inspect a message, open a long prompt or response, inspect related tool calls, and verify the memory item that was created from that interaction.', 'text-embedding-3-large', 0.78, '["migration","debuggability","agent-state"]', datetime('now', '-1 day'));

INSERT INTO settings (organization_id, setting_key, setting_value, setting_type, is_public, is_encrypted) VALUES
(1, 'theme', 'dark', 'string', 1, 0),
(1, 'billing.currency', 'USD', 'string', 0, 0),
(1, 'security.require_2fa', 'true', 'boolean', 0, 0),
(2, 'theme', 'light', 'string', 1, 0),
(3, 'analytics.retention_days', '365', 'integer', 0, 0),
(4, 'deployment.region', 'us-west-2', 'string', 0, 0),
(10, 'logistics.dispatch_cutoff', '17:00', 'string', 0, 0),
(11, 'healthcare.hipaa_mode', 'true', 'boolean', 0, 0);

UPDATE categories
SET product_count = (
    SELECT COUNT(*)
    FROM products
    WHERE products.category_id = categories.id
);

UPDATE tags
SET usage_count = (
    SELECT COUNT(*)
    FROM product_tags
    WHERE product_tags.tag_id = tags.id
);
