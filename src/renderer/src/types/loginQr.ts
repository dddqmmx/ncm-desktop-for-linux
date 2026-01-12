interface LoginQrKey {
  code: number
  data: {
    code: number
    unikey: string
  }
}

interface LoginQrCreate {
  code: number
  data: {
    qrurl: string
    qrimg: string
  }
}

interface LoginQrCheck {
  code: number
  message: string
  cookie: string
}
