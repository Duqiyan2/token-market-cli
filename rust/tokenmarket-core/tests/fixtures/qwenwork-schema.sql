CREATE TABLE `byok_custom_models` (
	`key` text PRIMARY KEY NOT NULL,
	`legacy_key` text,
	`source` text DEFAULT 'byok' NOT NULL,
	`display_name` text NOT NULL,
	`provider` text,
	`type` text,
	`url` text,
	`style` text,
	`model` text NOT NULL,
	`format` text,
	`is_vl` integer DEFAULT false NOT NULL,
	`is_reasoning` integer DEFAULT false NOT NULL,
	`max_input_tokens` integer DEFAULT 0 NOT NULL,
	`max_output_tokens` integer DEFAULT 0 NOT NULL,
	`encrypted_parameters` text DEFAULT '{}' NOT NULL,
	`extra_params` text DEFAULT '' NOT NULL,
	`migrated_from` text,
	`created_at` integer,
	`updated_at` integer
);
CREATE TABLE app_settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
