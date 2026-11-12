export interface LoginQrKey {
  code: number
  data: {
    code: number
    unikey: string
  }
}

export interface LoginQrCreate {
  code: number
  data: {
    qrurl: string
    qrimg: string
  }
}

export interface LoginQrCheck {
  code: number
  message: string
  cookie: string
}
