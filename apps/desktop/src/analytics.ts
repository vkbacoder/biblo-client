
/**
 * Product analytics disabled.
 * Keep the existing interface so callers remain compatible.
 */

export type EventProps = Record<string, string | number | boolean>;

export function track(_name: string, _props?: EventProps): void {
  // Analytics disabled.
}
