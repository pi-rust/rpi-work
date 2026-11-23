import { initialSample } from '@workbench/initial-snapshot'

// Both an explicit test build and an explicit URL are required. The product
// alias always resolves to null; a query string cannot enable sample data.
export const sample = new URLSearchParams(window.location.search).get('fixture') === '1' ? initialSample : null
