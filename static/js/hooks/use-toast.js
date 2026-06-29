// @ts-check
import { useMemo } from 'preact/hooks';
import { showToast, showApiError } from '../components/toast.js';

/**
 * Returns a stable toast helper object.
 * @returns {{ success: (msg: string) => void, info: (msg: string) => void, warn: (msg: string) => void, error: (msg: string) => void, apiError: (err: any) => void }}
 */
export function useToast() {
  return useMemo(() => ({
    success:  (msg) => showToast(msg, { type: 'success' }),
    info:     (msg) => showToast(msg, { type: 'info' }),
    warn:     (msg) => showToast(msg, { type: 'warn' }),
    error:    (msg) => showToast(msg, { type: 'error' }),
    apiError: showApiError,
  }), []);
}
