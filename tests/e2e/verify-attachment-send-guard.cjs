// Compatibility entry point. Local-only/text-only consent was replaced with
// genuine contentBlocks sending and fail-closed capability checks. Keep callers
// on the complete current regression rather than asserting the retired behavior.
require('./verify-attachment-send.cjs');
