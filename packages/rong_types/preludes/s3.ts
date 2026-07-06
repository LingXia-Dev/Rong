// Irreducible TS for the s3 module: option/result object shapes parsed
// inline from JSObject in Rust (no backing struct). Authored at their origin.

/** Options for constructing an S3 client. */
export interface S3ClientOptions {
  /** AWS access key ID. */
  accessKeyId?: string;
  /** AWS secret access key. */
  secretAccessKey?: string;
  /** AWS session token (STS). */
  sessionToken?: string;
  /** AWS region. @default "us-east-1" */
  region?: string;
  /** Custom endpoint URL (for S3-compatible services). */
  endpoint?: string;
  /** Bucket name. */
  bucket?: string;
  /** Default ACL for uploads (e.g. "public-read"). */
  acl?: string;
  /** Use virtual-hosted-style URLs instead of path-style. @default false */
  virtualHostedStyle?: boolean;
}

/**
 * Options for presigning URLs.
 */
export interface S3PresignOptions {
  /** Expiration in seconds. @default 86400 (24 hours) */
  expiresIn?: number;
  /** HTTP method. @default "GET" */
  method?: "GET" | "PUT" | "DELETE";
}

/**
 * Options for write operations.
 */
export interface S3WriteOptions {
  /** Content-Type header. @default "application/octet-stream" */
  type?: string;
}

/**
 * Options for list operations.
 */
export interface S3ListOptions {
  /** Filter objects by key prefix. */
  prefix?: string;
  /** Maximum number of keys to return. */
  maxKeys?: number;
  /** Start listing after this key (for pagination). */
  startAfter?: string;
}

// ==================== Result Types ====================

/**
 * Object metadata returned by `stat()`.
 */
export interface S3StatResult {
  /** ETag of the object. */
  etag?: string;
  /** Last modified timestamp (ISO 8601 string). */
  lastModified?: string;
  /** Object size in bytes. */
  size: number;
  /** Content-Type of the object. */
  type?: string;
}

/**
 * Single object entry in a list result.
 */
export interface S3ListEntry {
  /** Object key. */
  key: string;
  /** Object size in bytes. */
  size: number;
  /** Last modified timestamp (ISO 8601 string). */
  lastModified: string;
  /** ETag of the object. */
  etag?: string;
}

/**
 * Result of a list operation.
 */
export interface S3ListResult {
  /** List of matching objects. */
  contents: S3ListEntry[];
  /** Whether there are more results (use `startAfter` to paginate). */
  isTruncated: boolean;
}

// ==================== S3File Interface ====================

