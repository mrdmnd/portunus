The website for portunuspl.us

# Develop:
If you haven't produced a virtualenv yet, make one:

    python3 -m venv env

Ensure you are in the env

    . env/bin/activate

Ensure dependencies are up-to-date

    pip install -t lib -r requirements.txt --upgrade --no-cache-dir

The -t lib flag copies the libraries into a lib folder, which is uploaded to App
Engine during deployment. The -r requirements.txt flag tells pip to install
everything from that file.

If you haven't installed google cloud SDK, install it:
    curl https://sdk.cloud.google.com | bash


To deploy, run

    gcloud app deploy


# Dev DB:
Create stuff, explore.

    $ python
    from portunus import db
    from portunus.models import *
    db.create_all()
    user_1 = User(username='syn', email='syn@demo.com', password='password')
    db.session.add(user_1)
    db.session.commit()
    # User.query.all()
    # User.query.first()
    # User.query.filter_by(username='syn')...
    user = User.query.first() # fetch first user
    dungeon_route_1 = DungeonRoute(title='lol', content='do the things!', user_id=user.id)
    db.session.add(dungeon_route_1)
    db.session.commit()

    db.drop_all()


should create site.db file (or whatever it's named in the code)