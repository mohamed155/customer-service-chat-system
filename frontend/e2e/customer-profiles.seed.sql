TRUNCATE audit_logs, conversations, customer_channel_identifiers, customers, tenant_memberships, tenant_invitations, tenants, users RESTART IDENTITY CASCADE;

INSERT INTO users (id, email, display_name) VALUES
  ('c0a00000-0000-0000-0000-000000000001', 'agent@customer-profiles.test', 'Agent User'),
  ('c0a00000-0000-0000-0000-000000000002', 'viewer@customer-profiles.test', 'Viewer User'),
  ('c0a00000-0000-0000-0000-000000000003', 'cross@customer-profiles.test', 'Cross-Tenant User');

INSERT INTO tenants (id, name, slug) VALUES
  ('20000000-0000-0000-0000-000000000001', 'Customer Tenant', 'customer-tenant'),
  ('20000000-0000-0000-0000-000000000002', 'Other Tenant', 'other-tenant');

INSERT INTO tenant_memberships (id, tenant_id, user_id, role) VALUES
  ('c0b00000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000001', 'c0a00000-0000-0000-0000-000000000001', 'agent'),
  ('c0b00000-0000-0000-0000-000000000002', '20000000-0000-0000-0000-000000000001', 'c0a00000-0000-0000-0000-000000000002', 'viewer'),
  ('c0b00000-0000-0000-0000-000000000003', '20000000-0000-0000-0000-000000000002', 'c0a00000-0000-0000-0000-000000000003', 'agent');

INSERT INTO customers (id, tenant_id, display_name, email, phone, created_at)
SELECT
  gen_random_uuid(),
  '20000000-0000-0000-0000-000000000001',
  'Customer ' || LPAD(n::text, 2, '0'),
  'cust' || LPAD(n::text, 2, '0') || '@test.com',
  '+1555' || LPAD(n::text, 7, '0'),
  now() - (n * interval '1 day')
FROM generate_series(1, 30) AS n;

INSERT INTO customers (id, tenant_id, display_name, email, phone, created_at)
VALUES ('c0c00000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000001', 'Sara Ali', 'sara@example.com', '+201001234567', now() - interval '10 days');

INSERT INTO customer_channel_identifiers (id, tenant_id, customer_id, channel, identifier)
VALUES ('c0d00000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000001', 'c0c00000-0000-0000-0000-000000000001', 'email', 'sara@example.com'),
       ('c0d00000-0000-0000-0000-000000000002', '20000000-0000-0000-0000-000000000001', 'c0c00000-0000-0000-0000-000000000001', 'whatsapp', '+201001234567');

INSERT INTO conversations (id, tenant_id, customer_id, channel, status, last_activity_at, created_at)
VALUES ('c0e00000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000001', 'c0c00000-0000-0000-0000-000000000001', 'web_chat', 'open', now() - interval '1 hour', now() - interval '2 hours'),
       ('c0e00000-0000-0000-0000-000000000002', '20000000-0000-0000-0000-000000000001', 'c0c00000-0000-0000-0000-000000000001', 'email', 'closed', now() - interval '1 day', now() - interval '2 days'),
       ('c0e00000-0000-0000-0000-000000000003', '20000000-0000-0000-0000-000000000001', 'c0c00000-0000-0000-0000-000000000001', 'whatsapp', 'open', now() - interval '5 minutes', now() - interval '1 hour');

INSERT INTO customers (id, tenant_id, display_name, email, phone, created_at)
VALUES ('c0c00000-0000-0000-0000-0000000000ff', '20000000-0000-0000-0000-000000000002', 'Foreign Customer', 'foreign@test.com', '+15550000999', now() - interval '5 days');
