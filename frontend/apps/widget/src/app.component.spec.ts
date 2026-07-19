import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { AppComponent } from './app.component';
import { WIDGET_API_BASE } from './core/widget-api.service';

describe('Widget shell', () => {
  it('renders', async () => {
    await TestBed.configureTestingModule({
      imports: [AppComponent],
      providers: [provideHttpClient(), { provide: WIDGET_API_BASE, useValue: 'http://test' }],
    }).compileComponents();
    expect(TestBed.createComponent(AppComponent)).toBeTruthy();
  });
});
