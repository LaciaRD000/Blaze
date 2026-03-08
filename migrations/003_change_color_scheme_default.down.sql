ALTER TABLE user_themes ALTER COLUMN color_scheme SET DEFAULT 'base16-ocean.dark';
UPDATE user_themes SET color_scheme = 'base16-ocean.dark' WHERE color_scheme = 'base16-eighties.dark';
