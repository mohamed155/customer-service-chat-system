import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideTaiga } from '@taiga-ui/core';
import { APP_CONFIG } from '../../../core/config/app-config';
import { CustomerDialogComponent, CustomerDialogMode } from './customer-dialog.component';

describe('CustomerDialogComponent', () => {
  const create = (mode: CustomerDialogMode = 'create') => {
    TestBed.configureTestingModule({
      imports: [CustomerDialogComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        {
          provide: APP_CONFIG,
          useValue: {
            apiBaseUrl: 'http://localhost:8080/api/v1',
            publicDashboardUrl: 'https://dashboard.example.com',
          },
        },
      ],
    });

    const fixture = TestBed.createComponent(CustomerDialogComponent);
    fixture.componentRef.setInput('mode', mode);
    fixture.detectChanges();
    return fixture;
  };

  it('renders create mode with empty form fields', async () => {
    const fixture = create('create');
    await TestBed.compileComponents();
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent;
    expect(text).toContain('New customer');
    expect(text).toContain('Display name');
    expect(text).toContain('Email');
    expect(text).toContain('Phone');
    expect(text).toContain('Channel identifiers');
    expect(text).toContain('Metadata');

    const form = fixture.debugElement.query(By.css('form'));
    expect(form).not.toBeNull();
    const submitBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((btn) => btn.textContent?.trim() === 'Create customer');
    expect(submitBtn).toBeTruthy();
  });

  it('renders edit mode with pre-filled data', async () => {
    TestBed.configureTestingModule({
      imports: [CustomerDialogComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        {
          provide: APP_CONFIG,
          useValue: {
            apiBaseUrl: 'http://localhost:8080/api/v1',
            publicDashboardUrl: 'https://dashboard.example.com',
          },
        },
      ],
    });

    const fixture = TestBed.createComponent(CustomerDialogComponent);
    fixture.componentRef.setInput('mode', 'edit');
    fixture.componentRef.setInput('customer', {
      id: 'cust-1',
      displayName: 'Sara Ali',
      email: 'sara@example.com',
      phone: '+201001234567',
      channels: ['email', 'whatsapp'],
      identifiers: [
        { id: 'id-1', channel: 'email' as const, identifier: 'sara@example.com' },
        { id: 'id-2', channel: 'whatsapp' as const, identifier: '+201001234567' },
      ],
      metadata: { plan: 'enterprise', region: 'EMEA' },
      createdAt: '2026-07-13T10:00:00Z',
      updatedAt: '2026-07-13T10:00:00Z',
    });
    await TestBed.compileComponents();
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent;
    expect(text).toContain('Edit customer');

    const displayNameInput = fixture.nativeElement.querySelector(
      'input[aria-label="Display name"]',
    ) as HTMLInputElement;
    expect(displayNameInput?.value).toBe('Sara Ali');

    const emailInput = fixture.nativeElement.querySelector(
      'input[aria-label="Email"]',
    ) as HTMLInputElement;
    expect(emailInput?.value).toBe('sara@example.com');
  });

  it('validates required display name on submit', async () => {
    const fixture = create('create');
    await TestBed.compileComponents();
    fixture.detectChanges();

    const submitBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((btn) => btn.textContent?.trim() === 'Create customer')!;
    submitBtn.click();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Display name is required');
  });

  it('emits close on cancel', async () => {
    const fixture = create('create');
    await TestBed.compileComponents();
    fixture.detectChanges();

    let closed = false;
    fixture.componentInstance.closeDialog.subscribe(() => (closed = true));

    const cancelBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((btn) => btn.textContent?.trim() === 'Cancel')!;
    cancelBtn.click();

    expect(closed).toBe(true);
  });

  it('disables submit when submitting', async () => {
    const fixture = create('create');
    fixture.componentRef.setInput('submitting', true);
    await TestBed.compileComponents();
    fixture.detectChanges();

    const submitBtn = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((btn) => btn.textContent?.trim() === 'Saving…')!;
    expect(submitBtn).toBeTruthy();
    expect((submitBtn as HTMLButtonElement).disabled).toBe(true);
  });

  it('provides all five channel options', async () => {
    const fixture = create('create');
    await TestBed.compileComponents();
    fixture.detectChanges();

    addIdentifier(fixture.componentInstance, 'email', '');
    fixture.detectChanges();

    const options = fixture.nativeElement.querySelectorAll(
      '.identifier-row select[aria-label="Channel"] option',
    ) as NodeListOf<HTMLOptionElement>;
    const values = Array.from(options).map((opt) => opt.value);
    expect(values).toEqual(['email', 'phone', 'web_chat', 'whatsapp', 'telegram']);
  });

  it('renders the identifier conflict text, never the API message', async () => {
    const fixture = create('create');
    fixture.componentRef.setInput('error', {
      code: 'conflict',
      message: 'Conflict',
      status: 409,
      details: [
        {
          field: 'identifiers',
          code: 'unique_violation',
          message: 'Identifier already held by Sara Ali',
        },
      ],
    });
    await TestBed.compileComponents();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain(
      'Another customer already uses one of these',
    );
    expect(fixture.nativeElement.textContent).not.toContain('Identifier already held by Sara Ali');
  });

  it('names the customer who holds a conflicting identifier from the data fields, not the message', async () => {
    const fixture = create('create');
    fixture.componentRef.setInput('error', {
      code: 'conflict',
      message: 'Conflict',
      status: 409,
      details: [
        {
          field: 'identifiers',
          code: 'identifier_conflict',
          message: 'TEXT FROM THE API',
          channel: 'whatsapp',
          identifier: '+15550000999',
          existing_customer_name: 'Existing Customer',
        },
      ],
    });
    await TestBed.compileComponents();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain(
      'Channel `whatsapp:+15550000999` is already held by `Existing Customer`',
    );
    expect(fixture.nativeElement.textContent).not.toContain('TEXT FROM THE API');
  });

  it('shows the generic text of the code when a conflict names no identifiers', async () => {
    const fixture = create('create');
    fixture.componentRef.setInput('error', {
      code: 'conflict',
      message: 'A conflict occurred while saving',
      status: 409,
      details: [],
    });
    await TestBed.compileComponents();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('The request could not be completed');
    expect(fixture.nativeElement.textContent).not.toContain('A conflict occurred while saving');
  });

  const addIdentifier = (
    component: CustomerDialogComponent,
    channel: string,
    identifier: string,
  ): void => {
    component['addIdentifier'](channel as never, identifier);
  };

  const addMetadata = (component: CustomerDialogComponent, key: string, value: string): void => {
    component['addMetadata'](key, value);
  };

  describe('messages of the form checks', () => {
    async function textAfter(setup: (fixture: ReturnType<typeof create>) => void): Promise<string> {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();
      setup(fixture);
      fixture.detectChanges();
      return fixture.nativeElement.textContent as string;
    }

    it('names the channel in a phone identifier message', async () => {
      const text = await textAfter((fixture) => {
        addIdentifier(fixture.componentInstance, 'whatsapp', 'abc');
        addIdentifier(fixture.componentInstance, 'phone', '12');
      });
      expect(text).toContain('WhatsApp channel must be a valid phone number (7-15 digits');
      expect(text).toContain('Phone channel must be a valid phone number (7-15 digits');
    });

    it('says what is wrong with an email identifier', async () => {
      const text = await textAfter((fixture) => {
        addIdentifier(fixture.componentInstance, 'email', 'not-an-email');
        addIdentifier(fixture.componentInstance, 'email', `${'a'.repeat(65)}@example.com`);
      });
      expect(text).toContain('Email channel identifiers must be a valid email address.');
      expect(text).toContain('Email local part must be at most 64 characters.');
    });
  });

  describe('identifiers', () => {
    it('maintains correct indices after removing a middle identifier', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      addIdentifier(fixture.componentInstance, 'email', 'a@test.com');
      addIdentifier(fixture.componentInstance, 'phone', '+1');
      addIdentifier(fixture.componentInstance, 'whatsapp', '+2');
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelectorAll('.identifier-row').length).toBe(3);

      const removeBtns = fixture.nativeElement.querySelectorAll('.identifier-row app-icon-button');
      (removeBtns[1] as HTMLElement).click();
      fixture.detectChanges();

      const rows = fixture.nativeElement.querySelectorAll('.identifier-row');
      expect(rows.length).toBe(2);

      const identifiers = fixture.componentInstance['form'].controls.identifiers;
      expect(identifiers.at(0)?.value.identifier).toBe('a@test.com');
      expect(identifiers.at(1)?.value.identifier).toBe('+2');
    });

    it('includes duplicate channel identifiers in the submitted payload', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      let emitted: unknown = null;
      fixture.componentInstance.create.subscribe((p) => (emitted = p));

      const nameInput = fixture.nativeElement.querySelector(
        'input[aria-label="Display name"]',
      ) as HTMLInputElement;
      nameInput.value = 'Sara Ali';
      nameInput.dispatchEvent(new Event('input'));

      addIdentifier(fixture.componentInstance, 'email', 'sara@work.com');
      addIdentifier(fixture.componentInstance, 'email', 'sara@personal.com');
      fixture.detectChanges();

      const submitBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Create customer')!;
      submitBtn.click();
      fixture.detectChanges();

      expect(emitted).toMatchObject({
        displayName: 'Sara Ali',
        identifiers: [
          { channel: 'email', identifier: 'sara@work.com' },
          { channel: 'email', identifier: 'sara@personal.com' },
        ],
      });
    });

    it('adds a new identifier row when clicking add', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      const addBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === '+ Add identifier')!;
      expect(addBtn).toBeTruthy();

      addBtn.click();
      fixture.detectChanges();

      const rows = fixture.nativeElement.querySelectorAll('.identifier-row');
      expect(rows.length).toBe(1);
    });

    it('removes an identifier row when clicking remove', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      addIdentifier(fixture.componentInstance, 'email', 'test@example.com');
      addIdentifier(fixture.componentInstance, 'phone', '+201001234567');
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelectorAll('.identifier-row').length).toBe(2);

      const removeBtns = fixture.nativeElement.querySelectorAll('.identifier-row app-icon-button');
      (removeBtns[0] as HTMLElement).click();
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelectorAll('.identifier-row').length).toBe(1);
    });
  });

  describe('metadata', () => {
    it('adds a new metadata row when clicking add', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      const addBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === '+ Add metadata')!;
      expect(addBtn).toBeTruthy();

      addBtn.click();
      fixture.detectChanges();

      const rows = fixture.nativeElement.querySelectorAll('.metadata-row');
      expect(rows.length).toBe(1);
    });

    it('removes a metadata row when clicking remove', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      addMetadata(fixture.componentInstance, 'key1', 'value1');
      addMetadata(fixture.componentInstance, 'key2', 'value2');
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelectorAll('.metadata-row').length).toBe(2);

      const removeBtns = fixture.nativeElement.querySelectorAll('.metadata-row app-icon-button');
      (removeBtns[0] as HTMLElement).click();
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelectorAll('.metadata-row').length).toBe(1);
    });

    it('shows count in the legend', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).toContain('Metadata (0/50)');

      addMetadata(fixture.componentInstance, 'plan', 'enterprise');
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).toContain('Metadata (1/50)');
    });

    it('enforces the 50-entry limit and shows a warning', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      for (let i = 0; i < 51; i++) {
        addMetadata(fixture.componentInstance, `key${i}`, `value${i}`);
      }
      fixture.detectChanges();

      expect(fixture.nativeElement.querySelectorAll('.metadata-row').length).toBe(50);
      expect(fixture.nativeElement.textContent).toContain('Maximum 50 metadata entries.');
    });
  });

  describe('form submission', () => {
    it('emits full create payload with all fields including identifiers and metadata', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      let emitted: unknown = null;
      fixture.componentInstance.create.subscribe((p) => (emitted = p));

      const nameInput = fixture.nativeElement.querySelector(
        'input[aria-label="Display name"]',
      ) as HTMLInputElement;
      nameInput.value = 'Sara Ali';
      nameInput.dispatchEvent(new Event('input'));

      const emailInput = fixture.nativeElement.querySelector(
        'input[aria-label="Email"]',
      ) as HTMLInputElement;
      emailInput.value = 'sara@example.com';
      emailInput.dispatchEvent(new Event('input'));

      const phoneInput = fixture.nativeElement.querySelector(
        'input[aria-label="Phone"]',
      ) as HTMLInputElement;
      phoneInput.value = '+201001234567';
      phoneInput.dispatchEvent(new Event('input'));

      addIdentifier(fixture.componentInstance, 'email', 'sara@example.com');
      addIdentifier(fixture.componentInstance, 'whatsapp', '+201001234567');
      addMetadata(fixture.componentInstance, 'plan', 'enterprise');
      fixture.detectChanges();

      const submitBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Create customer')!;
      submitBtn.click();
      fixture.detectChanges();

      expect(emitted).toEqual({
        displayName: 'Sara Ali',
        email: 'sara@example.com',
        phone: '+201001234567',
        identifiers: [
          { channel: 'email', identifier: 'sara@example.com' },
          { channel: 'whatsapp', identifier: '+201001234567' },
        ],
        metadata: { plan: 'enterprise' },
      });
    });

    it('emits correct CreateCustomerPayload in create mode', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      let emitted: unknown = null;
      fixture.componentInstance.create.subscribe((p) => (emitted = p));

      const nameInput = fixture.nativeElement.querySelector(
        'input[aria-label="Display name"]',
      ) as HTMLInputElement;
      nameInput.value = 'Sara Ali';
      nameInput.dispatchEvent(new Event('input'));
      fixture.detectChanges();

      const emailInput = fixture.nativeElement.querySelector(
        'input[aria-label="Email"]',
      ) as HTMLInputElement;
      emailInput.value = 'sara@example.com';
      emailInput.dispatchEvent(new Event('input'));
      fixture.detectChanges();

      const submitBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Create customer')!;
      submitBtn.click();
      fixture.detectChanges();

      expect(emitted).toEqual({
        displayName: 'Sara Ali',
        email: 'sara@example.com',
      });
    });

    it('emits updated display name when modified in edit mode', async () => {
      const fixture = create('edit');
      fixture.componentRef.setInput('customer', {
        id: 'cust-1',
        displayName: 'Sara Ali',
        email: 'sara@example.com',
        phone: '+201001234567',
        channels: ['email', 'whatsapp'],
        identifiers: [
          { id: 'id-1', channel: 'email' as const, identifier: 'sara@example.com' },
          { id: 'id-2', channel: 'whatsapp' as const, identifier: '+201001234567' },
        ],
        metadata: { plan: 'enterprise', region: 'EMEA' },
        createdAt: '2026-07-13T10:00:00Z',
        updatedAt: '2026-07-13T10:00:00Z',
      });
      await TestBed.compileComponents();
      fixture.detectChanges();

      let emitted: unknown = null;
      fixture.componentInstance.update.subscribe((p) => (emitted = p));

      const nameInput = fixture.nativeElement.querySelector(
        'input[aria-label="Display name"]',
      ) as HTMLInputElement;
      nameInput.value = 'Sara Ali Updated';
      nameInput.dispatchEvent(new Event('input'));
      fixture.detectChanges();

      const submitBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Save changes')!;
      submitBtn.click();
      fixture.detectChanges();

      expect(emitted).toEqual({ displayName: 'Sara Ali Updated' });
    });

    it('emits only changed fields in edit mode with null for cleared contact fields', async () => {
      const fixture = create('edit');
      fixture.componentRef.setInput('customer', {
        id: 'cust-1',
        displayName: 'Sara Ali',
        email: 'sara@example.com',
        phone: '+201001234567',
        channels: ['email', 'whatsapp'],
        identifiers: [
          { id: 'id-1', channel: 'email' as const, identifier: 'sara@example.com' },
          { id: 'id-2', channel: 'whatsapp' as const, identifier: '+201001234567' },
        ],
        metadata: { plan: 'enterprise' },
        createdAt: '2026-07-13T10:00:00Z',
        updatedAt: '2026-07-13T10:00:00Z',
      });
      await TestBed.compileComponents();
      fixture.detectChanges();

      let emitted: unknown = null;
      fixture.componentInstance.update.subscribe((p) => (emitted = p));

      const emailInput = fixture.nativeElement.querySelector(
        'input[aria-label="Email"]',
      ) as HTMLInputElement;
      emailInput.value = '';
      emailInput.dispatchEvent(new Event('input'));
      fixture.detectChanges();

      const phoneInput = fixture.nativeElement.querySelector(
        'input[aria-label="Phone"]',
      ) as HTMLInputElement;
      phoneInput.value = '';
      phoneInput.dispatchEvent(new Event('input'));
      fixture.detectChanges();

      const submitBtn = Array.from(
        fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
      ).find((btn) => btn.textContent?.trim() === 'Save changes')!;
      submitBtn.click();
      fixture.detectChanges();

      expect(emitted).toEqual({
        email: null,
        phone: null,
      });
    });
  });

  describe('server error display', () => {
    it('shows error on the correct identifier row when indexed position is specified', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      addIdentifier(fixture.componentInstance, 'email', 'first@test.com');
      addIdentifier(fixture.componentInstance, 'phone', 'second');
      fixture.detectChanges();

      fixture.componentRef.setInput('error', {
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        details: [
          {
            field: 'identifiers[1].identifier',
            code: 'invalid',
            message: 'Invalid phone format',
          },
        ],
      });
      fixture.detectChanges();

      const rows = fixture.nativeElement.querySelectorAll('.identifier-row');
      expect(rows.length).toBe(2);
      const secondRow = rows[1] as HTMLElement;
      expect(secondRow.textContent).toContain('This value is not valid.');
      expect(secondRow.textContent).not.toContain('Invalid phone format');
      const firstRow = rows[0] as HTMLElement;
      expect(firstRow.textContent).not.toContain('This value is not valid.');
    });

    it('shows field-level errors on simple fields', async () => {
      const fixture = create('create');
      await TestBed.compileComponents();
      fixture.detectChanges();

      fixture.componentRef.setInput('error', {
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        details: [{ field: 'displayName', code: 'too_short', message: 'Name is too short' }],
      });
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).toContain('This value is not valid.');
      expect(fixture.nativeElement.textContent).not.toContain('Name is too short');
    });

    it('shows errors on identifier sub-controls', async () => {
      const fixture = create('create');
      addIdentifier(fixture.componentInstance, 'email', 'invalid');
      await TestBed.compileComponents();
      fixture.detectChanges();

      fixture.componentRef.setInput('error', {
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        details: [
          {
            field: 'identifiers[0].channel',
            code: 'invalid',
            message: 'Unsupported channel',
          },
        ],
      });
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).toContain('This value is not valid.');
      expect(fixture.nativeElement.textContent).not.toContain('Unsupported channel');
    });

    it('shows errors on metadata value controls', async () => {
      const fixture = create('create');
      addMetadata(fixture.componentInstance, 'plan', 'too-long-value');
      await TestBed.compileComponents();
      fixture.detectChanges();

      fixture.componentRef.setInput('error', {
        code: 'validation_failed',
        message: 'Validation failed',
        status: 422,
        details: [
          {
            field: 'metadata[plan]',
            code: 'too_long',
            message: 'Metadata plan value exceeds limit',
          },
        ],
      });
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).toContain('This value is not valid.');
      expect(fixture.nativeElement.textContent).not.toContain('Metadata plan value exceeds limit');
    });
  });
});
