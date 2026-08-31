/**
 * Phone-number handling.
 *
 * The backend validates E.164 strictly, so the app's job is to make entry
 * painless: accept whatever the user types, remember the dialling code, and
 * only send a canonical string.
 */

export interface Country {
  code: string;
  dial: string;
  name: string;
  flag: string;
  /** Digits after the dialling code, used only to nudge formatting. */
  nationalLength: number;
}

/** A working set; the picker searches the full list at runtime. */
export const COUNTRIES: Country[] = [
  { code: 'NG', dial: '+234', name: 'Nigeria', flag: '🇳🇬', nationalLength: 10 },
  { code: 'GH', dial: '+233', name: 'Ghana', flag: '🇬🇭', nationalLength: 9 },
  { code: 'KE', dial: '+254', name: 'Kenya', flag: '🇰🇪', nationalLength: 9 },
  { code: 'ZA', dial: '+27', name: 'South Africa', flag: '🇿🇦', nationalLength: 9 },
  { code: 'EG', dial: '+20', name: 'Egypt', flag: '🇪🇬', nationalLength: 10 },
  { code: 'GB', dial: '+44', name: 'United Kingdom', flag: '🇬🇧', nationalLength: 10 },
  { code: 'US', dial: '+1', name: 'United States', flag: '🇺🇸', nationalLength: 10 },
  { code: 'CA', dial: '+1', name: 'Canada', flag: '🇨🇦', nationalLength: 10 },
  { code: 'IN', dial: '+91', name: 'India', flag: '🇮🇳', nationalLength: 10 },
  { code: 'AE', dial: '+971', name: 'United Arab Emirates', flag: '🇦🇪', nationalLength: 9 },
  { code: 'DE', dial: '+49', name: 'Germany', flag: '🇩🇪', nationalLength: 11 },
  { code: 'FR', dial: '+33', name: 'France', flag: '🇫🇷', nationalLength: 9 },
  { code: 'BR', dial: '+55', name: 'Brazil', flag: '🇧🇷', nationalLength: 11 },
  { code: 'ID', dial: '+62', name: 'Indonesia', flag: '🇮🇩', nationalLength: 11 },
  { code: 'PK', dial: '+92', name: 'Pakistan', flag: '🇵🇰', nationalLength: 10 },
];

export const DEFAULT_COUNTRY = COUNTRIES[0];

/**
 * Builds the E.164 string the API expects.
 *
 * Strips a leading zero from the national part: people in most of the world
 * write their number as 0801… and the trunk prefix is not part of E.164.
 * Getting this wrong is the most common reason a valid number is rejected.
 */
export function toE164(country: Country, nationalNumber: string): string {
  const digits = nationalNumber.replace(/\D/g, '').replace(/^0+/, '');
  return `${country.dial}${digits}`;
}

export function isPlausible(country: Country, nationalNumber: string): boolean {
  const digits = nationalNumber.replace(/\D/g, '').replace(/^0+/, '');
  // Deliberately loose: numbering plans vary, and the server is the authority.
  // This only stops obviously incomplete entries from hitting the network.
  return digits.length >= 6 && digits.length <= 14;
}

/** Light grouping as the user types. Purely cosmetic. */
export function formatAsYouType(value: string): string {
  const digits = value.replace(/\D/g, '');
  const groups = digits.match(/.{1,3}/g);
  return groups ? groups.join(' ') : digits;
}

/** Presentation form for a stored E.164 number. */
export function prettyE164(e164: string): string {
  const country = COUNTRIES.find((candidate) => e164.startsWith(candidate.dial));
  if (!country) return e164;
  const rest = e164.slice(country.dial.length);
  return `${country.dial} ${formatAsYouType(rest)}`;
}
