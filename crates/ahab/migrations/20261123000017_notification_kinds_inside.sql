-- Kinds of notification shown only inside Ahab (its toasts and inbox): never
-- as the system's notifications or pushed to phones. Absent: everywhere
-- (while "outside the app" is on).
CREATE TABLE IF NOT EXISTS notification_kinds_inside (
    kind TEXT NOT NULL PRIMARY KEY
);
