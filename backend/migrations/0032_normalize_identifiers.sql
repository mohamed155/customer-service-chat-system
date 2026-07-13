-- Migration 0032: Normalize existing identifier rows to canonical form.
--
-- Ensures any pre-normalization rows (from earlier development) match the
-- app-side normalization rules (R3 in research.md):
--   - Trim all identifiers
--   - Lowercase email-channel identifiers
--   - E.164-normalize phone/WhatsApp identifiers (strip non-digit characters
--     except a single leading +)
--
-- Also normalizes the customers.phone contact field to E.164.
--
-- This migration is idempotent — WHERE clauses skip already-canonical rows.
--
-- WARNING: Collision-safe. Before the normalization UPDATEs, any rows that
-- would collide (same tenant_id + channel + normalized identifier) are
-- resolved by soft-deleting newer duplicates, keeping only the earliest row
-- per collision group. The subsequent UPDATEs then apply without violating
-- the partial unique index from migration 0029.

-- Step 0: Resolve collisions that normalization would create.
-- Within each (tenant_id, channel) group, identify rows whose post-normalization
-- identifier would collide. Soft-delete the newer duplicates, keeping the row
-- with the earliest created_at.
DO $$
DECLARE
  coll RECORD;
BEGIN
  FOR coll IN
    WITH normalized AS (
      SELECT
        id,
        tenant_id,
        channel,
        CASE
          WHEN channel = 'email' THEN LOWER(TRIM(identifier))
          WHEN channel IN ('phone', 'whatsapp') THEN '+' || REGEXP_REPLACE(TRIM(identifier), '[^0-9]', '', 'g')
          ELSE TRIM(identifier)
        END AS norm_identifier,
        created_at,
        ROW_NUMBER() OVER (
          PARTITION BY
            tenant_id,
            channel,
            CASE
              WHEN channel = 'email' THEN LOWER(TRIM(identifier))
              WHEN channel IN ('phone', 'whatsapp') THEN '+' || REGEXP_REPLACE(TRIM(identifier), '[^0-9]', '', 'g')
              ELSE TRIM(identifier)
            END
          ORDER BY created_at ASC, id ASC
        ) AS rn
      FROM customer_channel_identifiers
      WHERE deleted_at IS NULL
    )
    SELECT id FROM normalized WHERE rn > 1
  LOOP
    UPDATE customer_channel_identifiers
    SET deleted_at = NOW()
    WHERE id = coll.id;
  END LOOP;
END $$;

-- Step 1: Trim whitespace from all identifiers.
UPDATE customer_channel_identifiers
SET identifier = TRIM(identifier)
WHERE identifier <> TRIM(identifier);

-- Step 2: Lowercase email-channel identifiers.
UPDATE customer_channel_identifiers
SET identifier = LOWER(identifier)
WHERE channel = 'email' AND identifier <> LOWER(identifier);

-- Step 3: Normalize phone/WhatsApp identifiers to E.164 (+digits only).
UPDATE customer_channel_identifiers
SET identifier =
    '+' || REGEXP_REPLACE(identifier, '[^0-9]', '', 'g')
WHERE channel IN ('phone', 'whatsapp')
  AND identifier !~ '^\+\d+$';

-- Step 4: Normalize the customers.phone contact field to E.164.
UPDATE customers
SET phone =
    '+' || REGEXP_REPLACE(phone, '[^0-9]', '', 'g')
WHERE phone IS NOT NULL
  AND phone !~ '^\+\d+$';
