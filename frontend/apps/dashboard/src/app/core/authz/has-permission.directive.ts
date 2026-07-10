import { Directive, effect, inject, input, TemplateRef, ViewContainerRef } from '@angular/core';
import { Permission } from './permissions';
import { PermissionsService } from './permissions.service';

@Directive({
  selector: '[appHasPermission]',
  standalone: true,
})
export class HasPermissionDirective {
  private readonly templateRef = inject(TemplateRef);
  private readonly viewContainer = inject(ViewContainerRef);
  private readonly permissionsService = inject(PermissionsService);

  readonly appHasPermission = input.required<Permission>();

  constructor() {
    effect(() => {
      const hasPermission = this.permissionsService.has(this.appHasPermission());
      if (hasPermission) {
        this.viewContainer.createEmbeddedView(this.templateRef);
      } else {
        this.viewContainer.clear();
      }
    });
  }
}
