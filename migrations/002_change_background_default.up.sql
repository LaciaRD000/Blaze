ALTER TABLE user_themes ALTER COLUMN background_id SET DEFAULT 'gradient';
UPDATE user_themes SET background_id = 'gradient' WHERE background_id = 'default';
