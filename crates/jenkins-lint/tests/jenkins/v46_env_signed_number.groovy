pipeline {
    agent any
    environment {
        OFFSET = -1
        RATIO = -0.5
        STEP = +2
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
