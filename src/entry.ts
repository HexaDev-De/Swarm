// Keep the launcher isolated: importing main loads the full workspace and its listeners.
if (new URLSearchParams(location.search).get('view') === 'launcher') {
  void import('./overlay');
} else {
  void import('./main');
}
