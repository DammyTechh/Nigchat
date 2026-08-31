/**
 * Base64 with correct Unicode handling.
 *
 * The browser has `btoa`/`atob`, but they operate on Latin-1: passing them a
 * string containing an emoji or Yoruba diacritics throws
 * `InvalidCharacterError`. Going through `TextEncoder` first makes the round
 * trip byte-accurate, and keeps the web client byte-compatible with the mobile
 * one — both must produce the same ciphertext for the same message.
 */

export function encodeBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = '';
  // Chunked: spreading a large array into String.fromCharCode blows the call
  // stack somewhere above ~100k bytes.
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

export function decodeBase64(encoded: string): string {
  const binary = atob(encoded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return new TextDecoder().decode(bytes);
}
