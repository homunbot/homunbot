-- Remove cron_jobs table — all scheduling now unified under automations.
DROP TABLE IF EXISTS cron_jobs;
