import { Routes } from '@angular/router';
import { APP_PATHS } from '../../core/router/app-paths';

export const AUTH_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.auth.login },
  {
    path: APP_PATHS.auth.login,
    loadComponent: () => import('./login/login.component').then((m) => m.LoginComponent),
    title: 'Sign in',
  },
  {
    path: APP_PATHS.auth.signup,
    loadComponent: () => import('./signup/signup.component').then((m) => m.SignupComponent),
    title: 'Create account',
  },
  {
    path: APP_PATHS.auth.forgotPassword,
    loadComponent: () =>
      import('./forgot-password/forgot-password.component').then((m) => m.ForgotPasswordComponent),
    title: 'Forgot password',
  },
  {
    path: APP_PATHS.auth.verifyEmail,
    loadComponent: () =>
      import('./verify-email/verify-email.component').then((m) => m.VerifyEmailComponent),
    title: 'Verify email',
  },
];
