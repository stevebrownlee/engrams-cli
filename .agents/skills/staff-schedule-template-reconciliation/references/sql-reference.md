# Staff schedule template reconciliation — SQL reference

Use with prod `DB_URL` / `psql`. Replace `$EMAIL` and timezone as needed.

## User + role check

```sql
SELECT u.id, u.email, u.full_name, u.timezone_key, p.business_role_key, p.is_current
FROM core.users u
JOIN core.user_business_role_periods p ON p.user_id = u.id AND NOT p.is_deleted
WHERE lower(u.email) = lower('$EMAIL')
  AND p.is_current;
```

## Template series summary (any role)

```sql
SELECT s.ext->>'source' AS source,
       s.ext->>'weekday' AS weekday,
       s.ext->>'byday' AS byday,
       s.ext->>'start_time' AS start_time,
       s.ext->>'end_time' AS end_time,
       s.ext->>'off_day' AS off_day,
       s.ext->>'timezone_key' AS series_tz,
       s.ext->>'source_label' AS label,
       s.type_key,
       s.id AS series_id
FROM core.appointment_series s
JOIN core.appointment_series_logical_participants lp ON lp.appointment_series_id = s.id
JOIN core.users u ON u.id = lp.user_id
WHERE lower(u.email) = lower('$EMAIL')
  AND s.is_template AND NOT s.is_deleted
ORDER BY s.ext->>'source', s.ext->>'weekday', s.ext->>'start_time';
```

## HG weekday rollup

```sql
SELECT s.ext->>'weekday' AS weekday,
       count(*) AS blocks,
       string_agg(DISTINCT s.ext->>'source_label', ', ' ORDER BY s.ext->>'source_label') AS labels
FROM core.appointment_series s
JOIN core.appointment_series_logical_participants lp ON lp.appointment_series_id = s.id
JOIN core.users u ON u.id = lp.user_id
WHERE lower(u.email) = lower('$EMAIL')
  AND s.is_template AND NOT s.is_deleted
  AND s.ext->>'source' = 'virtual_hg_schedule_csv'
GROUP BY s.ext->>'weekday'
ORDER BY CASE s.ext->>'weekday'
  WHEN 'Monday' THEN 1 WHEN 'Tuesday' THEN 2 WHEN 'Wednesday' THEN 3
  WHEN 'Thursday' THEN 4 WHEN 'Friday' THEN 5 WHEN 'Saturday' THEN 6 WHEN 'Sunday' THEN 7 END;
```

## RN open-time series (single row expected)

```sql
SELECT u.timezone_key AS user_tz,
       s.ext->>'timezone_key' AS series_tz,
       s.ext->>'byday' AS byday,
       s.ext->>'off_day' AS off_day,
       s.ext->>'start_time' AS start_time,
       s.ext->>'end_time' AS end_time,
       s.type_key
FROM core.appointment_series s
JOIN core.appointment_series_logical_participants lp ON lp.appointment_series_id = s.id
JOIN core.users u ON u.id = lp.user_id
WHERE lower(u.email) = lower('$EMAIL')
  AND s.is_template AND NOT s.is_deleted
  AND s.ext->>'source' = 'one_time_rn_schedule_repair_2026_06';
```

## Materialized template appointments — wall clock in staff TZ

Correct conversion for naive UTC storage:

```sql
SELECT to_char(d, 'Dy') AS dow,
       to_char(a.scheduled_start_at AT TIME ZONE 'UTC' AT TIME ZONE 'America/Denver', 'HH24:MI') AS start_local,
       to_char(a.scheduled_end_at AT TIME ZONE 'UTC' AT TIME ZONE 'America/Denver', 'HH24:MI') AS end_local,
       a.title, a.type_key
FROM core.appointments a
JOIN core.appointment_series s ON s.id = a.appointment_series_id
JOIN core.appointment_series_logical_participants lp ON lp.appointment_series_id = s.id
JOIN core.users u ON u.id = lp.user_id
CROSS JOIN LATERAL (SELECT a.scheduled_start_at AT TIME ZONE 'UTC' AT TIME ZONE 'America/Denver') AS t(d)
WHERE lower(u.email) = lower('$EMAIL')
  AND a.is_template AND NOT a.is_deleted
  AND a.scheduled_start_at > now()
ORDER BY a.scheduled_start_at
LIMIT 20;
```

Verify off days: no occurrences on that weekday. Verify hours: start/end match
config in staff timezone.

## HG cohort counts

```sql
SELECT count(*) AS hg_template_series
FROM core.appointment_series s
WHERE s.is_template AND NOT s.is_deleted
  AND s.ext->>'source' = 'virtual_hg_schedule_csv';
```

## Series anchor vs materialized (debug timezone)

```sql
SELECT s.scheduled_start_at,
       s.scheduled_start_at AT TIME ZONE 'UTC' AT TIME ZONE 'America/Denver' AS anchor_start_local,
       s.scheduled_end_at AT TIME ZONE 'UTC' AT TIME ZONE 'America/Denver' AS anchor_end_local
FROM core.appointment_series s
JOIN core.appointment_series_logical_participants lp ON lp.appointment_series_id = s.id
JOIN core.users u ON u.id = lp.user_id
WHERE lower(u.email) = lower('$EMAIL') AND s.is_template AND NOT s.is_deleted;
```
