import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { provideTaiga } from '@taiga-ui/core';
import { VerifyEmailComponent } from './verify-email.component';

describe('VerifyEmailComponent', () => {
  beforeEach(() =>
    TestBed.configureTestingModule({
      imports: [VerifyEmailComponent],
      providers: [provideRouter([]), provideTaiga(), provideZonelessChangeDetection()],
    }),
  );

  it('renders six OTP inputs', async () => {
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(VerifyEmailComponent);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelectorAll('.otp input').length).toBe(6);
  });
});
