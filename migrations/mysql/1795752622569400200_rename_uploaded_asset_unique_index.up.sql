CREATE UNIQUE INDEX `uq_uploaded_assets_user_file_md5` ON `uploaded_assets` (`user_id`, `file_md5`);
DROP INDEX `uq_uploaded_assets_user_md5` ON `uploaded_assets`;
