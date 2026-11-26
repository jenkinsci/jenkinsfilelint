pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo "x $env.BRANCH_NAME. y"
            }
        }
    }
}
