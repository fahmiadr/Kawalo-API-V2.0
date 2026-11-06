import type { Failure } from '@core/error/failure';

export class Result<T, E extends Failure = Failure> {
  private constructor(
    private readonly _isSuccess: boolean,
    private readonly _value?: T,
    private readonly _failure?: E,
  ) {}

  public static ok<T, E extends Failure = Failure>(value: T): Result<T, E> {
    return new Result<T, E>(true, value, undefined);
  }

  public static fail<T, E extends Failure = Failure>(failure: E): Result<T, E> {
    return new Result<T, E>(false, undefined, failure);
  }

  public isSuccess(): boolean {
    return this._isSuccess;
  }

  public isFailure(): boolean {
    return !this._isSuccess;
  }

  public get value(): T {
    if (!this._isSuccess) {
      throw new Error('Cannot retrieve value from a failed Result. Check isSuccess() first.');
    }
    return this._value as T;
  }

  public get failure(): E {
    if (this._isSuccess) {
      throw new Error('Cannot retrieve failure from a successful Result. Check isFailure() first.');
    }
    return this._failure as E;
  }

  /**
   * Pattern Matching / Fold (Functional Monad).
   * Mengeksekusi handler onSuccess atau onFailure secara tipe-aman tanpa melempar runtime exception.
   */
  public fold<R>(onSuccess: (value: T) => R, onFailure: (failure: E) => R): R {
    return this._isSuccess ? onSuccess(this._value as T) : onFailure(this._failure as E);
  }

  /**
   * Transformasi nilai sukses dengan memetakan nilai `T` ke `U`.
   */
  public map<U>(fn: (value: T) => U): Result<U, E> {
    if (this._isSuccess) {
      return Result.ok<U, E>(fn(this._value as T));
    }
    return Result.fail<U, E>(this._failure as E);
  }

  /**
   * Monadic Chain / flatMap untuk merangkai operasi Result bertingkat.
   */
  public flatMap<U>(fn: (value: T) => Result<U, E>): Result<U, E> {
    if (this._isSuccess) {
      return fn(this._value as T);
    }
    return Result.fail<U, E>(this._failure as E);
  }

  /**
   * Mengambil nilai sukses atau mengembalikan fallback defaultValue jika Result gagal.
   */
  public getOrElse(defaultValue: T): T {
    return this._isSuccess ? (this._value as T) : defaultValue;
  }
}
