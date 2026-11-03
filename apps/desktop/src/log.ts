/**
 * Lightweight logging facade for the desktop UI.
 * Logs locally to the console only.
 * Never log secrets — command names, timings, and IDs only.
 */

type Fields = Record<string, unknown>;

export const log = {
  debug(message: string, fields?: Fields) {
    console.debug(`[debug] ${message}`, fields ?? "");
  },

  info(message: string, fields?: Fields) {
    console.info(`[info] ${message}`, fields ?? "");
  },

  warn(message: string, fields?: Fields) {
    console.warn(`[warn] ${message}`, fields ?? "");
  },

  error(message: string, err?: unknown, fields?: Fields) {
    console.error(`[error] ${message}`, err ?? "", fields ?? "");
  },
};