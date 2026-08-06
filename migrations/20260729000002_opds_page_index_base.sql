-- Default OPDS-PSE pageNumber substitution to one-based indexing while retaining
-- an operator setting for clients that send zero-based values.
ALTER TABLE settings ADD COLUMN opds_page_index_zero_based BOOLEAN NOT NULL DEFAULT 0;
