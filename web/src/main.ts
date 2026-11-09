// Entry point: routes between create, view and delete based on the URL.
import { ApiError, deletePaste } from './api';
import { showCreate } from './views/create';
import { showDeleteGate, showError, showLoading } from './views/gates';
import { showPaste } from './views/view';

const PASTE_ID = /^[a-f0-9]{16}$/;

function pasteIdFromSearch(search: string): string | null {
  // Identifiers are lower-case hex; accept links whose case was changed along the way.
  const query = search.replace(/^\?/, '').toLowerCase();
  if (!query) return null;
  const params = new URLSearchParams(query);
  const explicit = params.get('pasteid');
  if (explicit && PASTE_ID.test(explicit)) return explicit;
  const bare = query.split('&')[0];
  return PASTE_ID.test(bare) ? bare : null;
}

async function removePaste(id: string, token: string): Promise<void> {
  showLoading('Deleting…');
  try {
    await deletePaste(id, token);
    history.replaceState(null, '', location.pathname);
    showError('deleted');
  } catch (err) {
    showError('generic', { detail: err instanceof ApiError ? err.message : undefined, title: 'Could not delete the paste' });
  }
}

function route(): void {
  const params = new URLSearchParams(location.search);
  const id = params.get('pasteid');
  const token = params.get('deletetoken');
  if (id && token) {
    if (PASTE_ID.test(id.toLowerCase())) showDeleteGate(() => void removePaste(id.toLowerCase(), token));
    else showError('badlink');
    return;
  }
  const pasteId = pasteIdFromSearch(location.search);
  if (pasteId) {
    void showPaste(pasteId);
    return;
  }
  // A link with a key but no usable ID was most likely cut or edited on the way.
  if (location.search.length > 1 && location.hash.length > 1) {
    showError('badlink');
    return;
  }
  showCreate();
}

if (!window.isSecureContext || !crypto.subtle) {
  showError('generic', { detail: 'Encryption needs a secure connection. Open this page over HTTPS.', title: 'HTTPS required' });
} else {
  window.addEventListener('popstate', route);
  window.addEventListener('hashchange', route);
  route();
}
