// Client for the PrivateBin JSON API v2.
import type { CommentAdata, PasteAdata } from './crypto';

const HEADERS = { 'X-Requested-With': 'JSONHttpRequest', 'Content-Type': 'application/json' };

export class ApiError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ApiError';
  }
}

export interface CommentRecord {
  id: string;
  pasteid: string;
  parentid: string;
  v: number;
  adata: CommentAdata;
  ct: string;
  meta: { created?: number; icon?: string };
}

export interface PasteRecord {
  id: string;
  v: number;
  adata: PasteAdata;
  ct: string;
  meta: { created?: number; time_to_live?: number };
  comments: CommentRecord[];
  comment_count: number;
}

export interface CreatedPaste {
  id: string;
  url: string;
  deletetoken: string;
}

/** Base URL of this instance (works when served under a sub-path). */
export function baseUrl(): string {
  return location.origin + location.pathname;
}

async function call<T>(input: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(input, { ...init, headers: HEADERS, credentials: 'same-origin', cache: 'no-store' });
  } catch {
    throw new ApiError('The server could not be reached. Check your connection and try again.');
  }
  let body: { status?: number; message?: string } & Record<string, unknown>;
  try {
    body = await response.json();
  } catch {
    throw new ApiError(`Unexpected server response (HTTP ${response.status}).`);
  }
  if (body.status !== 0) throw new ApiError(body.message || 'Unknown server error.');
  return body as unknown as T;
}

export function createPaste(body: { v: 2; adata: PasteAdata; ct: string; meta: { expire: string } }): Promise<CreatedPaste> {
  return call<CreatedPaste>(baseUrl(), { method: 'POST', body: JSON.stringify(body) });
}

export function getPaste(id: string): Promise<PasteRecord> {
  return call<PasteRecord>(`${baseUrl()}?${encodeURIComponent(id)}`);
}

export function postComment(body: { v: 2; adata: CommentAdata; ct: string; pasteid: string; parentid: string }): Promise<{ id: string }> {
  return call<{ id: string }>(baseUrl(), { method: 'POST', body: JSON.stringify(body) });
}

export function deletePaste(id: string, token: string): Promise<{ id: string }> {
  return call<{ id: string }>(baseUrl(), { method: 'POST', body: JSON.stringify({ pasteid: id, deletetoken: token }) });
}
