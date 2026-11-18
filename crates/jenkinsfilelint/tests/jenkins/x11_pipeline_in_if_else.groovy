if (env.BRANCH_NAME == 'main') {
    pipeline {
        agent any
        stages {
            stage('S') {
                steps { echo 'x' }
            }
        }
    }
} else {
    pipeline {
        agent any
        stages {
            stage('S') {
                steps { echo 'y' }
            }
        }
    }
}
