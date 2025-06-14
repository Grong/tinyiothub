export const isTokenV1 = (token: Record<string, any>) => {
  return !token.version
}

export const getInitialTokenV2 = (): Record<string, any> => ({
  version: 2,
})

export const setAccessToken = async (sharedToken: string, token: string, user_id?: string) => {
  const accessToken = localStorage.getItem('token') || JSON.stringify(getInitialTokenV2())
  let accessTokenJson = getInitialTokenV2()
  try {
    accessTokenJson = JSON.parse(accessToken)
    if (isTokenV1(accessTokenJson))
      accessTokenJson = getInitialTokenV2()
  }
  catch {

  }

  accessTokenJson[sharedToken] = {
    ...accessTokenJson[sharedToken],
    [user_id || 'DEFAULT']: token,
  }
  localStorage.setItem('token', JSON.stringify(accessTokenJson))
}

export const removeAccessToken = () => {
  const sharedToken = globalThis.location.pathname.split('/').slice(-1)[0]

  const accessToken = localStorage.getItem('token') || JSON.stringify(getInitialTokenV2())
  let accessTokenJson = getInitialTokenV2()
  try {
    accessTokenJson = JSON.parse(accessToken)
    if (isTokenV1(accessTokenJson))
      accessTokenJson = getInitialTokenV2()
  }
  catch {

  }

  delete accessTokenJson[sharedToken]
  localStorage.setItem('token', JSON.stringify(accessTokenJson))
}
