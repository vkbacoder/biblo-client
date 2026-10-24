// Product analytics disabled for this client build.
export type EventProps = Record<string, string | number | boolean>;

export function track(_name: string, _props?: EventProps): void {
  // Intentionally disabled: no analytics events are transmitted.
}
