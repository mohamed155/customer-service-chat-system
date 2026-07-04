import { TestBed } from '@angular/core/testing';
import { AppComponent } from './app.component';
describe('Widget shell', () => {
  it('renders', async () => {
    await TestBed.configureTestingModule({ imports: [AppComponent] }).compileComponents();
    expect(TestBed.createComponent(AppComponent)).toBeTruthy();
  });
});
