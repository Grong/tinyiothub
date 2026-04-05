// Simple navigation helper - navigates by pushing to history
// Use this in page components for internal links
export function navigate(path: string) {
  history.pushState({}, '', path)
  // Dispatch a popstate event to trigger router navigation
  window.dispatchEvent(new PopStateEvent('popstate'))
}
