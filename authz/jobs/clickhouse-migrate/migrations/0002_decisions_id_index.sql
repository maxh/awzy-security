-- Looking up one decision by ID (explain-decision) skips granules that can't contain it.
ALTER TABLE decisions ADD INDEX id_bloom id TYPE bloom_filter GRANULARITY 4;
