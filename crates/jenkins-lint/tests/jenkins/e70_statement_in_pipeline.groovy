pipeline {
    agent any
    def x = 1
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
