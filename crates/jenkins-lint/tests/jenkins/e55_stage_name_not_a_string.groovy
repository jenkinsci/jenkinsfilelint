pipeline {
    agent any
    stages {
        stage(env.STAGE_NAME) {
            steps { echo 'x' }
        }
    }
}
