/**
 * Base64 for React Native.
 *
 * Hermes — the JS engine both platforms ship — has no `btoa`/`atob`. Using them
 * throws, which in this app meant every message body silently failed to decode
 * and rendered as raw base64. There is no global to polyfill safely across
 * Expo Go and a dev build, so the codec is implemented here.
 *
 * Unicode-safe: the message layer encodes UTF-8 bytes, not code units, so
 * emoji and non-Latin scripts survive the round trip. A naive
 * `btoa(unescape(encodeURIComponent(...)))` chain is the usual source of
 * mangled Yoruba and Arabic text.
 */

const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

function utf8Encode(input: string): number[] {
  const bytes: number[] = [];
  for (let i = 0; i < input.length; i += 1) {
    let code = input.charCodeAt(i);

    if (code < 0x80) {
      bytes.push(code);
    } else if (code < 0x800) {
      bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    } else if (code >= 0xd800 && code <= 0xdbff) {
      // Surrogate pair — one astral character (emoji) spread across two code
      // units. Combine them before encoding or the result is invalid UTF-8.
      const next = input.charCodeAt(i + 1);
      code = 0x10000 + ((code - 0xd800) << 10) + (next - 0xdc00);
      i += 1;
      bytes.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    } else {
      bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
    }
  }
  return bytes;
}

function utf8Decode(bytes: number[]): string {
  let output = '';
  let i = 0;

  while (i < bytes.length) {
    const byte = bytes[i];

    if (byte < 0x80) {
      output += String.fromCharCode(byte);
      i += 1;
    } else if (byte >= 0xc0 && byte < 0xe0) {
      output += String.fromCharCode(((byte & 0x1f) << 6) | (bytes[i + 1] & 0x3f));
      i += 2;
    } else if (byte >= 0xe0 && byte < 0xf0) {
      output += String.fromCharCode(
        ((byte & 0x0f) << 12) | ((bytes[i + 1] & 0x3f) << 6) | (bytes[i + 2] & 0x3f),
      );
      i += 3;
    } else {
      const code =
        (((byte & 0x07) << 18) |
          ((bytes[i + 1] & 0x3f) << 12) |
          ((bytes[i + 2] & 0x3f) << 6) |
          (bytes[i + 3] & 0x3f)) -
        0x10000;
      output += String.fromCharCode(0xd800 + (code >> 10), 0xdc00 + (code & 0x3ff));
      i += 4;
    }
  }

  return output;
}

export function encodeBase64(text: string): string {
  const bytes = utf8Encode(text);
  let output = '';

  for (let i = 0; i < bytes.length; i += 3) {
    const chunk = (bytes[i] << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    const remaining = bytes.length - i;

    output += ALPHABET[(chunk >> 18) & 63];
    output += ALPHABET[(chunk >> 12) & 63];
    output += remaining > 1 ? ALPHABET[(chunk >> 6) & 63] : '=';
    output += remaining > 2 ? ALPHABET[chunk & 63] : '=';
  }

  return output;
}

export function decodeBase64(encoded: string): string {
  const clean = encoded.replace(/[^A-Za-z0-9+/]/g, '');
  const bytes: number[] = [];

  for (let i = 0; i < clean.length; i += 4) {
    const chunk =
      (ALPHABET.indexOf(clean[i]) << 18) |
      (ALPHABET.indexOf(clean[i + 1]) << 12) |
      ((clean[i + 2] ? ALPHABET.indexOf(clean[i + 2]) : 0) << 6) |
      (clean[i + 3] ? ALPHABET.indexOf(clean[i + 3]) : 0);

    bytes.push((chunk >> 16) & 0xff);
    if (clean[i + 2]) bytes.push((chunk >> 8) & 0xff);
    if (clean[i + 3]) bytes.push(chunk & 0xff);
  }

  return utf8Decode(bytes);
}
