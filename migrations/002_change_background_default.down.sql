ALTER TABLE user_themes ALTER COLUMN background_id SET DEFAULT 'default';
UPDATE user_themes SET background_id = 'default' WHERE background_id = 'gradient';
