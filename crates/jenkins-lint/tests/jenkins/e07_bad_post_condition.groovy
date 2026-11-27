pipeline {
    agent any
    post {
        sucess { echo 'x' }
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
