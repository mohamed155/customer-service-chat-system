-- Migration 0030: Cascade soft-delete from customers to channel identifiers
--
-- When a customer row is soft-deleted (deleted_at transitions from NULL to a
-- value), this trigger automatically soft-deletes all live channel identifiers
-- belonging to that customer.

CREATE OR REPLACE FUNCTION cascade_customer_soft_delete()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL THEN
    UPDATE customer_channel_identifiers
    SET deleted_at = NEW.deleted_at
    WHERE customer_id = OLD.id AND tenant_id = OLD.tenant_id AND deleted_at IS NULL;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_cascade_identifier_soft_delete
  BEFORE UPDATE OF deleted_at ON customers
  FOR EACH ROW
  EXECUTE FUNCTION cascade_customer_soft_delete();
