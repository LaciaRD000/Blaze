ALTER TABLE user_themes
    ALTER COLUMN show_line_numbers TYPE INTEGER USING (CASE WHEN show_line_numbers THEN 1 ELSE 0 END),
    ALTER COLUMN show_line_numbers SET DEFAULT 0;
