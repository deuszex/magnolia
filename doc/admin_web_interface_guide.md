# Admin guide to browser interface
![alt text](img/admin/admin_pages.bmp)

For admins only difference in the options is the "Admin Panel"

## User tab
![alt text](img/admin/users_tab.bmp)

## Registration applications
![alt text](img/admin/applications.bmp)

Registration Applications do not mean software applications, but one of the user registration modes, where instead of free registration, or invititation based registration, the user requests that they can register.

## Invitations
![alt text](img/admin/invites_tab.bmp)

Invites can be done with either emails, or by the admin giving an invite link in another way to the user. Invites can be tied to email addresses, so they can only be used with that email address.

## Frontend theme
![alt text](img/admin/theme.bmp)

Server side color scheme settings. Some text configuration for a tiny bit more personalization.

## Site config (server options)
![alt text](img/admin/site_config.bmp)

The most significant options are on this page. Registration mode, enabling other systems, media storage path.

## SMTP (emailing)
![alt text](img/admin/smtp.bmp)

SMTP Email settings.

## STUN/TURN Addresses
![alt text](img/admin/stun_turn.bmp)

External STUN/TURN server addresses, for webRTC (audio/video call).

## Server-to-Server Federation
![alt text](img/admin/federation_settings.bmp)

Server-to-server communication. Enabling gives the option to start connections with other servers, allowing users of different servers to communicate.

## Proxy system
![alt text](img/admin/admin_proxies.bmp)

The proxy system is for automation (bots). There are proxies that are tied to users (one per user) and proxies that are managed by admins. Proxies are rate-limited, with there being a server side limit, and a per proxy limit, and out of the two the lower is the one applied to the proxy. Both limited in file upload size, and connection numbers, both per minute.

There are two authentication options.
- Session based (same as regular users) with the caveat that there are separate login and logout endpoints for proxies.
- HMAC signed one-shots. The functionalities one would usually expect to use, like creating a post, or sending a message; available without creating a session. A key is created while in session as a user or admin for the proxy, that key can be used to sign the message.
