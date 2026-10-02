// Plug the browser mock into the bridge and the preview. Imported on demand by index.tsx,
// in mock mode only, so none of mock/ is part of the packaged app's startup bundle.
import { installMock } from "../bridge";
import { installMockPreview } from "../preview";
import { mockBackend } from "./backend";
import { MockPreviewClient } from "./preview";

installMock(mockBackend);
installMockPreview(() => new MockPreviewClient());
