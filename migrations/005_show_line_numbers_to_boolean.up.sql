ALTER TABLE user_themes
    ALTER COLUMN show_line_numbers TYPE BOOLEAN USING (show_line_numbers != 0),
    ALTER COLUMN show_line_numbers SET DEFAULT false;
