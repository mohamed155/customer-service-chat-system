import { CanMatchFn } from '@angular/router';

/** Authentication will replace this explicit access-control seam in its own feature spec. */
export const areaAccessGuard: CanMatchFn = () => true;
