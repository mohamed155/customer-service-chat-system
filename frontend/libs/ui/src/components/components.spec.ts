import { TestBed } from '@angular/core/testing';
import { BadgeComponent } from './badge.component';
import { ButtonComponent } from './button.component';
import { DialogComponent } from './dialog.component';
import { InputComponent } from './input.component';
import { SelectComponent } from './select.component';
import { TableComponent } from './table.component';
import { TabsComponent } from './tabs.component';
import { ToastComponent } from './toast.component';
const components = [
  ButtonComponent,
  InputComponent,
  SelectComponent,
  TableComponent,
  DialogComponent,
  ToastComponent,
  BadgeComponent,
  TabsComponent,
];
for (const component of components) {
  describe(component.name, () => {
    it('renders without error', async () => {
      await TestBed.configureTestingModule({ imports: [component] }).compileComponents();
      expect(TestBed.createComponent(component)).toBeTruthy();
    });
  });
}
